use super::{NativeOutput, Planner, PUBLICATIONS, SERVICES};
use crate::{
    config::Config,
    native_parameters::RuntimeParameters,
    platform::{self, Clock, SystemClock},
    Error,
};
use openpilot_logging::{
    log_site,
    producer::Factory,
    record::{Level, Record},
    runtime::RuntimeDiagnostics,
    Number,
};
use openpilot_messaging::runtime::{PubMaster, SubMaster};
use openpilot_params::Params;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

pub fn run(frames: Option<u64>, artifact: &Path) -> Result<(), Error> {
    platform::configure()?;
    let stop = Arc::new(AtomicBool::new(false));
    let post_params = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register_conditional_default(
        signal_hook::consts::SIGTERM,
        Arc::clone(&post_params),
    )?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let mut parameters = RuntimeParameters(Params::for_runtime()?);
    let mut logger = Factory::for_runtime()?.logger();
    logger.emit(
        log_site!(),
        Record::text(Level::Info, "plannerd is waiting for CarParams".into()),
    )?;
    let config = loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        let bytes = parameters.car_params()?;
        if !bytes.is_empty() {
            break Config::decode(&bytes)?;
        }
        thread::sleep(Duration::from_millis(100));
    };
    post_params.store(true, Ordering::SeqCst);
    logger.emit(
        log_site!(),
        Record::text(
            Level::Info,
            format!("plannerd got CarParams: {}", config.brand),
        ),
    )?;
    let (mut planner, (publisher, mut subscriber)) =
        Planner::load_with(config, artifact, &mut parameters, |options| {
            let publisher = PubMaster::for_runtime(&PUBLICATIONS)?;
            let subscriber = SubMaster::for_runtime(&SERVICES, options)?;
            Ok((publisher, subscriber))
        })?;
    let mut output = NativeOutput {
        publisher,
        logger,
        diagnostics: RuntimeDiagnostics::new("plannerd", 1.),
    };
    let clock = SystemClock;
    let mut count = 0;
    while !stop.load(Ordering::Relaxed) {
        let wait_started = clock.monotonic();
        subscriber.update(Duration::from_millis(100))?;
        let started = clock.monotonic();
        let cpu_started = clock.thread_cpu();
        let mut tick = planner.process(&subscriber.state, &mut parameters, &clock, &mut output)?;
        let context = tick.context();
        let mut measurements = vec![
            (
                "work_ms".into(),
                Number::Float((clock.monotonic() - started) * 1000.),
            ),
            (
                "thread_cpu_ms".into(),
                Number::Float((clock.thread_cpu() - cpu_started) * 1000.),
            ),
            (
                "model_updated".into(),
                Number::Integer(i64::from(tick.model_updated)),
            ),
            (
                "longitudinal_run".into(),
                Number::Integer(i64::from(tick.longitudinal_run)),
            ),
            (
                "poll_ms".into(),
                Number::Float((started - wait_started) * 1000.),
            ),
        ];
        measurements.append(&mut tick.timings);
        output
            .diagnostics
            .record(&mut output.logger, log_site!(), measurements, context)?;
        count += 1;
        if frames.is_some_and(|limit| count >= limit) {
            break;
        }
    }
    Ok(())
}
