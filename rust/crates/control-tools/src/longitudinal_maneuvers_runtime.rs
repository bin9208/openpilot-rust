use crate::{longitudinal_maneuvers::Controller, longitudinal_maneuvers_wire as wire, Error};
use openpilot_logging::{
    log_site,
    producer::Factory,
    record::{Level, Record},
};
use openpilot_messaging::{
    runtime::{PubMaster, SubMaster},
    state::{Options, Poll},
};
use openpilot_params::Params;
use std::{
    num::NonZeroU64,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

pub fn run(frames: Option<NonZeroU64>) -> Result<(), Error> {
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let params = Params::for_runtime()?;
    let mut logger = Factory::for_runtime()?.logger();
    logger.emit(
        log_site!(),
        Record::text(Level::Info, "joystickd is waiting for CarParams".into()),
    )?;
    let mut controller = loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        let bytes = match params.get("CarParams") {
            Ok(bytes) => bytes,
            Err(openpilot_params::Error::Io(_)) => None,
            Err(error) => return Err(error.into()),
        };
        if let Some(bytes) = bytes.filter(|bytes| !bytes.is_empty()) {
            break Controller::new(wire::stopping_speed(&bytes)?)?;
        }
        thread::sleep(Duration::from_millis(100));
    };
    let mut subscriber = SubMaster::for_runtime(
        &wire::TOPICS,
        Options {
            poll: Poll::One("modelV2".into()),
            ..Options::default()
        },
    )?;
    let mut publisher = PubMaster::for_runtime(&wire::OUTPUTS)?;
    let mut count = 0_u64;
    while !stop.load(Ordering::Relaxed) {
        let result = subscriber.update(Duration::from_secs(1));
        if stop.load(Ordering::Relaxed) {
            break;
        }
        result?;
        let input = wire::input(&subscriber.state)?;
        let command = controller.step(&input)?;
        for (name, bytes) in
            wire::OUTPUTS
                .iter()
                .zip(wire::encode(&command, input.valid, timestamp)?)
        {
            publisher.send(name, &bytes)?;
        }
        count = count.saturating_add(1);
        if frames.is_some_and(|limit| count >= limit.get()) {
            break;
        }
    }
    Ok(())
}

fn timestamp() -> Result<u64, Error> {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    u64::try_from(now.tv_sec)
        .ok()
        .and_then(|seconds| seconds.checked_mul(1_000_000_000))
        .and_then(|seconds| {
            u64::try_from(now.tv_nsec)
                .ok()
                .and_then(|nanos| seconds.checked_add(nanos))
        })
        .ok_or(Error::Contract("monotonic timestamp overflow"))
}
