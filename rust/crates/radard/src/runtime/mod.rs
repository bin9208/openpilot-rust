mod platform;
use crate::{
    daemon::{self, Radar, SERVICES},
    wire, Error,
};
use num_traits::ToPrimitive;
use openpilot_logging::{
    log_site,
    producer::Factory,
    record::{Level, Record},
    runtime::RuntimeDiagnostics,
    Fields, Number,
};
use openpilot_messaging::runtime::{PubMaster, SubMaster};
use openpilot_params::Params;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
fn raw(params: &Params, key: &str) -> Result<Vec<u8>, Error> {
    match params.get(key) {
        Ok(value) => Ok(value.unwrap_or_default()),
        Err(openpilot_params::Error::Io(_)) => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}
fn integer(params: &Params, key: &str) -> Result<i32, Error> {
    openpilot_beepd::integer(&raw(params, key)?)
        .map_err(|error| Error::Parameter(error.to_string()))
}
fn seconds(clock: rustix::time::ClockId) -> f64 {
    let now = rustix::time::clock_gettime(clock);
    now.tv_sec as f64 + now.tv_nsec as f64 / 1e9
}
fn monotonic() -> f64 {
    seconds(rustix::time::ClockId::Monotonic)
}
fn stamp() -> Result<u64, Error> {
    (monotonic() * 1e9)
        .to_u64()
        .ok_or(Error::Contract("event timestamp range"))
}

pub fn run(frames: Option<u64>) -> Result<(), Error> {
    platform::configure()?;
    let stop = Arc::new(AtomicBool::new(false));
    let post_params = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register_conditional_default(
        signal_hook::consts::SIGTERM,
        Arc::clone(&post_params),
    )?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let params = Params::for_runtime()?;
    let mut logger = Factory::for_runtime()?.logger();
    logger.emit(
        log_site!(),
        Record::text(Level::Info, "dPath radard is waiting for CarParams".into()),
    )?;
    let bytes = loop {
        if stop.load(Ordering::SeqCst) {
            return Err(Error::Signal(signal_hook::consts::SIGINT));
        }
        let bytes = raw(&params, "CarParams")?;
        if !bytes.is_empty() {
            break bytes;
        }
        thread::sleep(Duration::from_millis(100));
    };
    post_params.store(true, Ordering::SeqCst);
    logger.emit(
        log_site!(),
        Record::text(Level::Info, "dPath radard got CarParams".into()),
    )?;
    let mut subscriber = SubMaster::for_runtime(&SERVICES, daemon::options())?;
    let mut publisher = PubMaster::for_runtime(&["radarState"])?;
    let config = wire::configuration(
        &bytes,
        integer(&params, "EnableRadarTracks")?,
        integer(&params, "EnableCornerRadar")?,
    )?;
    let mut radar = Radar::new(config);
    let mut diagnostics = RuntimeDiagnostics::new("radard", 1.);
    let mut count = 0u64;
    while !stop.load(Ordering::SeqCst) {
        subscriber.update(Duration::from_millis(100))?;
        if !subscriber.state.topic("modelV2")?.updated {
            continue;
        }
        let started = monotonic();
        let cpu_started = seconds(rustix::time::ClockId::ThreadCPUTime);
        if let Some(bytes) = radar.process(&subscriber.state, stamp()?)? {
            publisher.send("radarState", &bytes)?;
        }
        diagnostics.record(
            &mut logger,
            log_site!(),
            [
                (
                    "work_ms".into(),
                    Number::Float((monotonic() - started) * 1000.),
                ),
                (
                    "thread_cpu_ms".into(),
                    Number::Float(
                        (seconds(rustix::time::ClockId::ThreadCPUTime) - cpu_started) * 1000.,
                    ),
                ),
                (
                    "model_age_ms".into(),
                    Number::Float(
                        (started - subscriber.state.topic("modelV2")?.log_mono_time as f64 * 1e-9)
                            * 1000.,
                    ),
                ),
                (
                    "tracks_age_ms".into(),
                    Number::Float(
                        (started
                            - subscriber.state.topic("liveTracks")?.log_mono_time as f64 * 1e-9)
                            * 1000.,
                    ),
                ),
            ],
            Fields::default(),
        )?;
        count = count
            .checked_add(1)
            .ok_or(Error::Contract("radard frame count overflow"))?;
        if frames.is_some_and(|limit| count >= limit) {
            return Ok(());
        }
    }
    Err(Error::Signal(signal_hook::consts::SIGINT))
}
