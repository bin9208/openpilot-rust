use crate::{bridge::ffi, loop_state::LoopState, types::Event, wire, Error};
use openpilot_logging::{
    log_site,
    producer::Factory,
    record::{Level, Record},
};
use openpilot_messaging::{
    runtime::{PubMaster, SubMaster},
    state::{Options, Poll},
};
use openpilot_msgq::Subscriber;
use std::{
    io::Write,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

pub fn monotonic() -> f64 {
    let value = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    value.tv_sec as f64 + value.tv_nsec as f64 * 1e-9
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
        .map_err(|_| Error::Contract("environment flag must be an integer"))?
        != 0)
}
fn subscribe(name: &str) -> Result<Subscriber, Error> {
    let service =
        openpilot_messaging::services::lookup(name).ok_or(Error::Contract("sensor service"))?;
    Ok(Subscriber::for_runtime(name, false, service.queue_size)?)
}
fn drain(socket: &mut Subscriber) -> Result<Vec<Event>, Error> {
    let mut events = Vec::new();
    while let Some(bytes) = socket.receive(Duration::ZERO)? {
        events.push(wire::decode(&bytes)?);
    }
    Ok(events)
}
pub fn run(mut remaining: Option<u64>) -> Result<(), Error> {
    ffi::configure_scheduler(!Path::new("/TICI").is_file())?;
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let mut publisher = PubMaster::for_runtime(&["livePose"])?;
    let mut subscriber = SubMaster::for_runtime(
        &["carState", "liveCalibration", "cameraOdometry"],
        Options {
            poll: Poll::One("cameraOdometry".into()),
            ..Options::default()
        },
    )?;
    let mut acceleration = subscribe("accelerometer")?;
    let mut gyroscope = subscribe("gyroscope")?;
    let params = openpilot_params::Params::for_runtime()?;
    let mut state = LoopState::new(flag("DEBUG")?, flag("SIMULATION")?)?;
    let mut logger = Factory::for_runtime()?.logger();
    if let Some(bytes) = params.get("LocationFilterInitialState")? {
        let seed = wire::seed(&bytes)?;
        state.estimator.kf.reset(None, &seed.x, &seed.covariance)?;
    }
    while !stop.load(Ordering::Relaxed) {
        subscriber.update(Duration::from_millis(100))?;
        let acceleration = drain(&mut acceleration)?;
        let gyroscope = drain(&mut gyroscope)?;
        let output = state.step(&subscriber.state, &acceleration, &gyroscope, monotonic)?;
        for (level, text) in std::mem::take(&mut state.estimator.logs) {
            let level = match level.as_str() {
                "warning" => Level::Warning,
                "error" => Level::Error,
                _ => return Err(Error::Contract("estimator log level")),
            };
            logger.emit(log_site!(), Record::text(level, text))?;
        }
        if let Some(output) = output {
            if let Some(diagnostic) = output.diagnostic {
                println!("{diagnostic}");
                std::io::stdout().flush()?;
            }
            publisher.send(
                "livePose",
                &wire::encode(&output.pose, (monotonic() * 1e9) as u64)?,
            )?;
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
