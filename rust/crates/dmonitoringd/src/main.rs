use openpilot_dmonitoringd::{
    Error,
    controller::{Controller, TOPICS},
};
use openpilot_messaging::{
    runtime::{PubMaster, SubMaster},
    state::{Options, Poll},
};
use openpilot_params::Params;
use std::{
    env,
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

fn options() -> Result<Option<Option<u64>>, Error> {
    let mut args = env::args_os().skip(1);
    let mut frames = None;
    while let Some(arg) = args.next() {
        if arg == "--help" {
            println!(
                "openpilot-dmonitoringd [--frames N]\n\nRuns the native DM2 dispatcher and camera/interaction monitoring policy continuously.\n--frames bounds 20 Hz monitoring publications. Uses OPENPILOT_PREFIX and PARAMS_ROOT."
            );
            return Ok(None);
        } else if arg == "--frames" && frames.is_none() {
            frames = Some(
                args.next()
                    .and_then(|value| value.to_str().and_then(|value| value.parse::<u64>().ok()))
                    .filter(|value| *value > 0)
                    .ok_or(Error::Contract("invalid frame count"))?,
            );
        } else {
            return Err(Error::Contract("unknown or duplicate option; see --help"));
        }
    }
    Ok(Some(frames))
}

fn configure() -> Result<(), Error> {
    if std::path::Path::new("/TICI").is_file() {
        // SAFETY: integer/timespec fields allow zero, including musl's additional sched_param fields.
        let mut settings: libc::sched_param = unsafe { std::mem::zeroed() };
        settings.sched_priority = 5;
        // SAFETY: sched_setscheduler borrows the initialized parameter for this call only.
        if unsafe { libc::sched_setscheduler(0, libc::SCHED_FIFO, &settings) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut cores = rustix::thread::CpuSet::new();
        for core in 0..4 {
            cores.set(core);
        }
        rustix::thread::sched_setaffinity(None, &cores).map_err(std::io::Error::from)?;
    }
    Ok(())
}

fn run(frames: Option<u64>) -> Result<(), Error> {
    configure()?;
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let params = Params::for_runtime()?;
    // Source waits for the current fingerprint onroad; Driver View stays nonblocking.
    let initial_car_params = loop {
        let value = params.get("CarParams")?;
        if value.is_some() || !params.get_bool("IsOnroad")? {
            break value;
        }
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut publisher = PubMaster::for_runtime(&["driverMonitoringState"])?;
    let mut subscriber = SubMaster::for_runtime(
        TOPICS,
        Options {
            poll: Poll::One(
                if env::var_os("REPLAY").is_some() {
                    "modelV2"
                } else {
                    "driverStateV2"
                }
                .into(),
            ),
            ..Options::default()
        },
    )?;
    let mut controller = Controller::new(&params)?;
    if let Some(bytes) = initial_car_params {
        let message = capnp::serialize::read_message(
            std::io::Cursor::new(bytes),
            capnp::message::ReaderOptions::new(),
        )?;
        controller.configure_car_params(
            message.get_root::<openpilot_cereal::car_capnp::car_params::Reader>()?,
            &params,
        )?;
    }
    let mut car_queue = openpilot_msgq::Subscriber::for_runtime(
        "carState",
        false,
        openpilot_messaging::services::lookup("carState")
            .unwrap()
            .queue_size,
    )?;
    let mut control_queue = openpilot_msgq::Subscriber::for_runtime(
        "carControl",
        false,
        openpilot_messaging::services::lookup("carControl")
            .unwrap()
            .queue_size,
    )?;
    let mut published = 0;
    eprintln!("dmonitoringd: ready");
    while !stop.load(Ordering::Relaxed) {
        let deadline = Instant::now() + Duration::from_millis(50);
        subscriber.update(Duration::from_millis(50))?;
        controller.refresh(&params)?;
        if subscriber.state.topic("carParams")?.updated {
            let openpilot_cereal::log_capnp::event::CarParams(cp) =
                subscriber.state.topic("carParams")?.event()?.which()?
            else {
                return Err(Error::Contract("expected carParams"));
            };
            controller.configure_car_params(cp?, &params)?;
        }
        while let Some(bytes) = control_queue.receive(Duration::ZERO)? {
            controller.control_packet(
                &bytes,
                openpilot_dmonitoringd::controller::runtime_clock(&subscriber.state),
            )?;
        }
        let mut car_packets = Vec::new();
        while let Some(bytes) = car_queue.receive(Duration::ZERO)? {
            car_packets.push(bytes);
        }
        // Drain automatic requests again after the car snapshot, matching the
        // source's cross-socket echo-race protection.
        while let Some(bytes) = control_queue.receive(Duration::ZERO)? {
            controller.control_packet(
                &bytes,
                openpilot_dmonitoringd::controller::runtime_clock(&subscriber.state),
            )?;
        }
        for bytes in car_packets {
            controller.car_packet(
                &bytes,
                openpilot_dmonitoringd::controller::runtime_clock(&subscriber.state),
                &params,
            )?;
        }
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if let Some(output) = controller.prepare(&subscriber.state)? {
            publisher.send("driverMonitoringState", &output.bytes)?;
            controller.after_publish(output.frame_id, &params)?;
            published += 1;
            if frames == Some(published) {
                break;
            }
        }
        // Forget missed deadlines: camera absence cannot create catch-up bursts.
        if let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            std::thread::sleep(remaining);
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match openpilot_managed_entry::runtime(
        "openpilot.selfdrive.monitoring.dm2d",
        "dmonitoringd",
        || {
            options()
                .and_then(|options| options.map_or(Ok(()), run))
                .map_err(openpilot_managed_entry::StepError::raised)
        },
    ) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("dmonitoringd: {error}");
            ExitCode::FAILURE
        }
    }
}
