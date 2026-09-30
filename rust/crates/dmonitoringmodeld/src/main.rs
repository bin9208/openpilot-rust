use openpilot_dmonitoringmodeld::{driver::Calibration, runtime::DriverRuntime, Error};
use openpilot_model_runtime::catalog::{Catalog, Kind};
use openpilot_modeld::driver_wire::DriverTiming;
use openpilot_msgq::{Publisher, Subscriber, VisionClient, VisionStream};
use std::{
    env,
    path::PathBuf,
    process::ExitCode,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

struct Options {
    catalog: PathBuf,
    frames: Option<u64>,
}

fn options() -> Result<Option<Options>, Error> {
    let mut args = env::args_os().skip(1);
    let mut catalog = None;
    let mut frames = None;
    while let Some(arg) = args.next() {
        if arg == "--help" {
            println!("openpilot-dmonitoringmodeld --trusted-catalog PATH [--frames N]\n\nRuns the driver VisionIPC/native-model/driverStateV2 loop. PATH must contain\ntrusted immutable executable model artifacts. Omit --frames to run continuously.\nUses the original OPENPILOT_PREFIX namespace and SEND_RAW_PRED behavior.");
            return Ok(None);
        } else if arg == "--trusted-catalog" && catalog.is_none() {
            catalog = Some(PathBuf::from(
                args.next()
                    .ok_or(Error::Contract("missing trusted catalog path"))?,
            ));
        } else if arg == "--frames" && frames.is_none() {
            let value = args.next().ok_or(Error::Contract("missing frame count"))?;
            let count: u64 = value
                .to_str()
                .ok_or(Error::Contract("invalid frame count"))?
                .parse()
                .map_err(|_| Error::Contract("invalid frame count"))?;
            if count == 0 {
                return Err(Error::Contract("frame count must be positive"));
            }
            frames = Some(count);
        } else {
            return Err(Error::Contract("unknown or duplicate option; see --help"));
        }
    }
    Ok(Some(Options {
        catalog: catalog.ok_or(Error::Contract("--trusted-catalog is required"))?,
        frames,
    }))
}

fn configure_realtime() -> Result<(), Error> {
    if std::path::Path::new("/TICI").is_file() {
        // SAFETY: sched_param contains only integers/timespecs; zero also initializes musl's extra fields.
        let mut settings: libc::sched_param = unsafe { std::mem::zeroed() };
        settings.sched_priority = 5;
        // SAFETY: sched_setscheduler borrows a valid parameter for this call only.
        if unsafe { libc::sched_setscheduler(0, libc::SCHED_FIFO, &settings) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut cores = rustix::thread::CpuSet::new();
        cores.set(7);
        rustix::thread::sched_setaffinity(None, &cores).map_err(std::io::Error::from)?;
    }
    Ok(())
}

fn timestamp() -> Result<u64, Error> {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    u64::try_from(now.tv_sec)
        .ok()
        .and_then(|seconds| seconds.checked_mul(1_000_000_000))
        .and_then(|seconds| seconds.checked_add(u64::try_from(now.tv_nsec).ok()?))
        .ok_or(Error::Contract("monotonic timestamp overflow"))
}

fn run(options: Options) -> Result<(), Error> {
    configure_realtime()?;
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let catalog = Catalog::load(&options.catalog)?;
    let raw = env::var_os("SEND_RAW_PRED").is_some_and(|value| !value.is_empty());
    let mut client = VisionClient::new("camerad", VisionStream::Driver, true)?;
    eprintln!("dmonitoringmodeld: connecting to driver stream");
    while !stop.load(Ordering::Relaxed) && !client.connect()? {
        thread::sleep(Duration::from_millis(100));
    }
    if stop.load(Ordering::Relaxed) {
        return Ok(());
    }
    eprintln!("dmonitoringmodeld: driver stream connected");
    let mut runtime = None;
    let mut subscriber = None;
    let mut publisher = None;
    let mut calibration = Calibration::default();
    let mut frames = 0;
    while !stop.load(Ordering::Relaxed) {
        if !client.is_connected() {
            thread::sleep(Duration::from_millis(100));
            continue;
        }
        let Some(frame) = client.receive(Duration::from_millis(100))? else {
            continue;
        };
        let metadata = *frame.metadata();
        if runtime.is_none() {
            let camera = [
                u32::try_from(metadata.width)
                    .map_err(|_| Error::Contract("camera width overflow"))?,
                u32::try_from(metadata.height)
                    .map_err(|_| Error::Contract("camera height overflow"))?,
            ];
            let bundle = catalog.select(Kind::Driver, camera)?;
            let priority = if bundle.backend == "qcom-cl" {
                env::var("QCOM_PRIORITY")
                    .map_or(Ok(8), |value| value.parse::<u8>())
                    .map_err(|_| Error::Contract("invalid QCOM_PRIORITY"))?
            } else {
                8
            };
            // SAFETY: --trusted-catalog explicitly requires immutable executable artifacts.
            runtime = Some(unsafe { DriverRuntime::load(bundle, priority) }?);
            subscriber = Some(Subscriber::for_runtime(
                "liveCalibration",
                true,
                1024 * 1024,
            )?);
            publisher = Some(Publisher::for_runtime("driverStateV2", 1024 * 1024)?);
            eprintln!("dmonitoringmodeld: models loaded");
        }
        let runtime = runtime
            .as_mut()
            .ok_or(Error::Contract("driver model not initialized"))?;
        let subscriber = subscriber
            .as_mut()
            .ok_or(Error::Contract("calibration subscriber not initialized"))?;
        let publisher = publisher
            .as_mut()
            .ok_or(Error::Contract("driver publisher not initialized"))?;
        if let Some(message) = subscriber.receive(Duration::ZERO)? {
            calibration.update(&message)?;
        }
        let start = Instant::now();
        let gpu_execution_time = runtime.infer(&frame, calibration.values())?;
        let model_execution_time = start.elapsed().as_secs_f32();
        let message = runtime.message(
            DriverTiming {
                log_mono_time: timestamp()?,
                frame_id: metadata.frame_id,
                model_execution_time,
                gpu_execution_time,
            },
            raw,
        )?;
        publisher.send(&message)?;
        frames += 1;
        if options.frames == Some(frames) {
            break;
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match options().and_then(|options| options.map_or(Ok(()), run)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("dmonitoringmodeld: {error}");
            ExitCode::FAILURE
        }
    }
}
