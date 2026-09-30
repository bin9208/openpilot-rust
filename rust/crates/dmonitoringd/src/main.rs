use openpilot_dmonitoringd::{
    controller::{Controller, TOPICS},
    Error,
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
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

fn options() -> Result<Option<Option<u64>>, Error> {
    let mut args = env::args_os().skip(1);
    let mut frames = None;
    while let Some(arg) = args.next() {
        if arg == "--help" {
            println!("openpilot-dmonitoringd [--frames N]\n\nRuns the original driver monitoring policy and cereal/Params loop continuously.\n--frames bounds driver-frame publications. Uses OPENPILOT_PREFIX and PARAMS_ROOT.");
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
    let mut publisher = PubMaster::for_runtime(&["driverMonitoringState"])?;
    let mut subscriber = SubMaster::for_runtime(
        TOPICS,
        Options {
            poll: Poll::One("driverStateV2".into()),
            ..Options::default()
        },
    )?;
    let mut controller = Controller::new(&params)?;
    let mut published = 0;
    eprintln!("dmonitoringd: ready");
    while !stop.load(Ordering::Relaxed) {
        subscriber.update(Duration::from_millis(100))?;
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
    }
    Ok(())
}

fn main() -> ExitCode {
    match options().and_then(|options| options.map_or(Ok(()), run)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("dmonitoringd: {error}");
            ExitCode::FAILURE
        }
    }
}
