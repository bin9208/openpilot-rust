use super::{io::monotonic, scheduler, Error, Monitor, NativeIo, RunOptions};
use crate::core::StepIo;
use openpilot_params::Params;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub fn run(options: RunOptions) -> Result<(), Error> {
    scheduler::configure()?;
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    let mut io = NativeIo::new(Params::for_runtime()?, Arc::clone(&stop))?;
    let initialized = super::initialize(
        &mut io,
        Params::for_runtime()?,
        Params::for_runtime()?,
        &options.root.join("opendbc_repo/opendbc/dbc"),
        &options.root.join("opendbc_repo/opendbc/car/torque_data"),
        &options.numerics,
    );
    if stop.load(Ordering::Relaxed) {
        return Err(Error::Interrupted);
    }
    let mut initialized = initialized?;
    io.start_params_reader(
        initialized
            .card
            .params
            .get_root_as_reader::<openpilot_cereal::car_capnp::car_params::Reader>()?
            .get_openpilot_longitudinal_control(),
    )?;
    let mut monitor = Monitor::default();
    let mut frequency = options
        .frequency_trace
        .as_deref()
        .map(super::frequency_trace::FrequencyTrace::open)
        .transpose()?;
    loop {
        if stop.load(Ordering::Relaxed) {
            return Err(Error::Interrupted);
        }
        initialized.card.remaining = monitor.remaining;
        let result =
            initialized
                .card
                .step(&mut initialized.vehicle, &mut initialized.tail, &mut io);
        if stop.load(Ordering::Relaxed) {
            return Err(Error::Interrupted);
        }
        result?;
        if let Some(trace) = &mut frequency {
            trace.record(super::frequency_trace::TraceInput {
                state: io.subscribers(),
                controls_ready: initialized.card.settings.get_bool("ControlsReady")?,
                parser: initialized.vehicle.parser_readiness(),
            })?;
        }
        monitor.monitor(monotonic);
        if options
            .max_steps
            .is_some_and(|limit| monitor.frames >= limit.get())
        {
            return Ok(());
        }
    }
}
