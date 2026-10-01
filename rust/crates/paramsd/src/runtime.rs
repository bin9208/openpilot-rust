use crate::{
    bridge::ffi,
    cache::{self, Store},
    estimator::Estimator,
    loop_state::LoopState,
    wire, Error,
};
use openpilot_logging::{
    log_site,
    producer::{Factory, Logger},
    record::{Level, Record},
};
use openpilot_messaging::{
    runtime::{PubMaster, SubMaster},
    state::{Options, Poll},
};
use openpilot_params::Params;
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub fn monotonic() -> u64 {
    let value = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    value.tv_sec as u64 * 1_000_000_000 + value.tv_nsec as u64
}
fn flag(name: &str) -> Result<bool, Error> {
    let value = match std::env::var(name) {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => "0".into(),
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(Error::Contract("non-UTF8 environment flag"))
        }
    };
    Ok(value
        .trim()
        .parse::<i64>()
        .map_err(|_| Error::Contract("environment flag must be integer"))?
        != 0)
}
struct PendingWrites {
    sender: Option<mpsc::Sender<Vec<u8>>>,
    worker: Option<JoinHandle<()>>,
}
impl PendingWrites {
    fn new(params: Params, key: &'static str) -> Result<Self, Error> {
        let (sender, receiver) = mpsc::channel::<Vec<u8>>();
        let worker = thread::Builder::new()
            .name(format!("paramsd-{key}"))
            .spawn(move || {
                while let Ok(bytes) = receiver.recv() {
                    let _ = params.put(key, &bytes);
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            worker: Some(worker),
        })
    }
    fn put(&self, bytes: Vec<u8>) -> Result<(), Error> {
        self.sender
            .as_ref()
            .ok_or(Error::Contract("closed Params writer"))?
            .send(bytes)
            .map_err(|_| Error::Contract("Params worker stopped"))
    }
}
impl Drop for PendingWrites {
    fn drop(&mut self) {
        self.sender.take();
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            eprintln!("paramsd: Params writer panicked");
        }
    }
}
fn emit(logger: &mut Logger, logs: &mut Vec<(String, String)>) -> Result<(), Error> {
    for (level, text) in std::mem::take(logs) {
        let level = match level.as_str() {
            "info" => Level::Info,
            "warning" => Level::Warning,
            "error" => Level::Error,
            _ => return Err(Error::Contract("log level")),
        };
        logger.emit(log_site!(), Record::text(level, text))?;
    }
    Ok(())
}
pub fn run(mut remaining: Option<u64>, memory_root: Option<PathBuf>) -> Result<(), Error> {
    ffi::configure_scheduler(!Path::new("/TICI").is_file())?;
    let debug = flag("DEBUG")?;
    let replay = flag("REPLAY")?;
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let mut publisher = PubMaster::for_runtime(&["liveParameters"])?;
    let mut params = Params::for_runtime()?;
    let gps = if Store::get(&mut params, "UbloxAvailable").as_deref() == Some(b"1") {
        "gpsLocationExternal"
    } else {
        "gpsLocation"
    };
    let mut subscriber = SubMaster::for_runtime(
        &["livePose", "liveCalibration", "carState", gps],
        Options {
            poll: Poll::One("livePose".into()),
            ignore_alive: vec![gps.into()],
            ignore_valid: vec![gps.into()],
            ..Options::default()
        },
    )?;
    let car = loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        if let Some(bytes) = Store::get(&mut params, "CarParams") {
            break wire::car(&bytes)?;
        }
        thread::sleep(Duration::from_millis(100));
    };
    let mut logger = Factory::for_runtime()?.logger();
    let mut logs = Vec::new();
    cache::migrate(&mut params, monotonic(), &mut logs);
    let initial = cache::retrieve(&mut params, &car, replay, debug, &mut logs);
    emit(&mut logger, &mut logs)?;
    let estimator = Estimator::new(
        &car,
        initial.ratio,
        initial.stiffness,
        initial.offset_degrees.to_radians(),
        initial.covariance,
    )?;
    let mut state = LoopState {
        estimator,
        gps_service: gps.into(),
        debug,
    };
    let memory_root = memory_root.unwrap_or_else(|| PathBuf::from("/dev/shm/params"));
    let prefix = std::env::var("OPENPILOT_PREFIX").unwrap_or_else(|_| "d".into());
    let mut memory = Params::open(&memory_root, &prefix)?;
    Store::remove(&mut memory, "LastGPSPosition");
    let memory = PendingWrites::new(memory, "LastGPSPosition")?;
    let persistent = PendingWrites::new(params, "LiveParametersV2")?;
    while !stop.load(Ordering::Relaxed) {
        subscriber.update(Duration::from_millis(100))?;
        let output = state.step(&subscriber.state, monotonic())?;
        emit(&mut logger, &mut state.estimator.logs)?;
        if let Some(bytes) = output.gps {
            memory.put(bytes)?;
        }
        if let Some(bytes) = output.packet {
            if output.cache {
                persistent.put(bytes.clone())?;
            }
            publisher.send("liveParameters", &bytes)?;
            if let Some(count) = &mut remaining {
                *count -= 1;
                if *count == 0 {
                    break;
                }
            }
        }
    }
    Ok(())
}
