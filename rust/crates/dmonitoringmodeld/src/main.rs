use openpilot_dmonitoringmodeld::{driver::Calibration, runtime::DriverRuntime, Error};
use openpilot_logging::{
    log_site,
    producer::{Factory, Logger},
    record::{Level, Record},
};
use openpilot_messaging::services;
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
    let mut logger = Factory::for_runtime()?.logger();
    let stop = Arc::new(AtomicBool::new(false));
    let interrupted = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&interrupted))?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let result = run_loop(options, &mut logger, &stop);
    if interrupted.load(Ordering::Relaxed) {
        logger.emit(
            log_site!(),
            Record::text(Level::Warning, "got SIGINT".into()),
        )?;
    }
    logger.close();
    result
}

// A new process reloads imports and model state after the established stream changes.
fn camera_ready(client: &VisionClient, stop: &AtomicBool) -> Result<bool, Error> {
    if stop.load(Ordering::Relaxed) {
        return Ok(false);
    }
    if !client.is_connected() {
        return Err(Error::Contract(
            "camera generation lost; restart dmonitoringmodeld",
        ));
    }
    Ok(true)
}

fn run_loop(options: Options, logger: &mut Logger, stop: &Arc<AtomicBool>) -> Result<(), Error> {
    let catalog = Catalog::load(&options.catalog)?;
    let raw = env::var_os("SEND_RAW_PRED").is_some_and(|value| !value.is_empty());
    logger.emit(
        log_site!(),
        Record::text(Level::Warning, "connecting to driver stream".into()),
    )?;
    let mut client = VisionClient::new("camerad", VisionStream::Driver, true)?;
    while !stop.load(Ordering::Relaxed) && !client.connect()? {
        thread::sleep(Duration::from_millis(100));
    }
    if stop.load(Ordering::Relaxed) {
        return Ok(());
    }
    logger.emit(
        log_site!(),
        Record::text(
            Level::Warning,
            format!(
                "connected with buffer size: {}",
                client
                    .layout()
                    .map_or_else(|| "None".into(), |layout| layout.len.to_string())
            ),
        ),
    )?;
    let layout = client
        .layout()
        .ok_or(Error::Contract("connected camera has no layout"))?;
    let camera = [
        u32::try_from(layout.width).map_err(|_| Error::Contract("camera width overflow"))?,
        u32::try_from(layout.height).map_err(|_| Error::Contract("camera height overflow"))?,
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
    let mut runtime = unsafe { DriverRuntime::load(bundle, priority) }?;
    logger.emit(
        log_site!(),
        Record::text(
            Level::Warning,
            "models loaded, dmonitoringmodeld starting".into(),
        ),
    )?;
    let mut subscriber = Subscriber::for_runtime(
        "liveCalibration",
        true,
        services::lookup("liveCalibration")
            .ok_or(Error::Contract("liveCalibration service missing"))?
            .queue_size,
    )?;
    let mut publisher = Publisher::for_runtime(
        "driverStateV2",
        services::lookup("driverStateV2")
            .ok_or(Error::Contract("driverStateV2 service missing"))?
            .queue_size,
    )?;
    let mut calibration = Calibration::default();
    let mut frames = 0;
    while !stop.load(Ordering::Relaxed) {
        if !camera_ready(&client, stop)? {
            break;
        }
        let Some(frame) = client.receive(Duration::from_millis(100))? else {
            if !camera_ready(&client, stop)? {
                break;
            }
            continue;
        };
        let metadata = *frame.metadata();
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
    match openpilot_managed_entry::runtime(
        "openpilot.selfdrive.modeld.dmonitoringmodeld",
        "dmonitoringmodeld",
        || {
            options()
                .and_then(|options| options.map_or(Ok(()), run))
                .map_err(openpilot_managed_entry::StepError::raised)
        },
    ) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("dmonitoringmodeld: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(all(test, not(feature = "visionipc-ion")))]
mod camera_tests {
    use super::*;
    use openpilot_msgq::{VisionLayout, VisionMetadata, VisionServer};

    #[test]
    fn driver_generation_change_is_terminal_but_timeout_and_stop_are_not() {
        if env::var_os("DRIVER_CAMERA_RESTART_CHILD").is_none() {
            let prefix = format!("driver-restart-{}", std::process::id());
            let directory = PathBuf::from("/dev/shm").join(format!("msgq_{prefix}"));
            std::fs::create_dir(&directory).unwrap();
            let status = std::process::Command::new(env::current_exe().unwrap())
                .args(["--exact", "camera_tests::driver_generation_change_is_terminal_but_timeout_and_stop_are_not", "--nocapture"])
                .env("DRIVER_CAMERA_RESTART_CHILD", "1")
                .env("OPENPILOT_PREFIX", prefix)
                .env_remove("CEREAL_FAKE")
                .status().unwrap();
            std::fs::remove_dir_all(directory).unwrap();
            assert!(status.success());
            return;
        }
        let layout = VisionLayout {
            width: 8,
            height: 8,
            stride: 8,
            uv_offset: 64,
            len: 96,
        };
        let server = VisionServer::new("driver-restart").unwrap();
        let images = server
            .create_stream(VisionStream::Driver, 1, layout)
            .unwrap();
        server.start_listener().unwrap();
        let mut client = VisionClient::new("driver-restart", VisionStream::Driver, true).unwrap();
        assert!(client.connect().unwrap());
        let stop = AtomicBool::new(false);
        assert!(camera_ready(&client, &stop).unwrap());
        assert!(client.receive(Duration::ZERO).unwrap().is_none());
        assert!(camera_ready(&client, &stop).unwrap());
        drop(images);
        drop(server);
        let server = VisionServer::new("driver-restart").unwrap();
        let layout = VisionLayout {
            width: 16,
            height: 8,
            stride: 16,
            uv_offset: 128,
            len: 192,
        };
        let images = server
            .create_stream(VisionStream::Driver, 1, layout)
            .unwrap();
        server.start_listener().unwrap();
        let metadata = VisionMetadata {
            width: layout.width,
            height: layout.height,
            stride: layout.stride,
            uv_offset: layout.uv_offset,
            len: layout.len,
            frame_id: 2,
            timestamp_sof: 2,
            timestamp_eof: 3,
            valid: true,
            received: true,
            index: 0,
            fd: -1,
        };
        // Queue publication may first expose the producer's generation reset.
        for _ in 0..10 {
            images[0].publish(metadata).unwrap();
            assert!(client
                .receive(Duration::from_millis(100))
                .unwrap()
                .is_none());
            if !client.is_connected() {
                break;
            }
        }
        assert!(
            camera_ready(&client, &stop).is_err(),
            "new server generation must exit driver daemon"
        );
        stop.store(true, Ordering::Relaxed);
        assert!(
            !camera_ready(&client, &stop).unwrap(),
            "shutdown must be graceful"
        );
        let mut fresh = VisionClient::new("driver-restart", VisionStream::Driver, true).unwrap();
        assert!(fresh.connect().unwrap());
        assert_eq!(fresh.layout(), Some(layout));
        images[0].publish(metadata).unwrap();
        assert_eq!(
            fresh
                .receive(Duration::from_millis(100))
                .unwrap()
                .unwrap()
                .metadata()
                .len,
            192
        );
        let mut invalid =
            Publisher::for_runtime("visionipc_driver-restart_1", 1024 * 1024).unwrap();
        let mut packet = [0_u8; 48];
        packet[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
        for _ in 0..10 {
            invalid.send(&packet).unwrap();
            assert!(fresh.receive(Duration::from_millis(100)).unwrap().is_none());
            if !fresh.is_connected() {
                break;
            }
        }
        assert!(
            camera_ready(&fresh, &AtomicBool::new(false)).is_err(),
            "invalid slot must exit driver daemon"
        );
    }
}
