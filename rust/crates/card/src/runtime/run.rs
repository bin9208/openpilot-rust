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
        &options.dbc,
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
        .map(|path| {
            super::frequency_trace::FrequencyTrace::open(
                path,
                options.fixture_phase_fence.clone(),
                options.max_steps.map_or(u64::MAX, |limit| limit.get()),
            )
        })
        .transpose()?;
    loop {
        if stop.load(Ordering::Relaxed) {
            return Err(Error::Interrupted);
        }
        initialized.card.remaining = monitor.remaining;
        let started = frequency
            .as_ref()
            .map(|_| (super::io::monotonic_ns(), io.thread_cpu_ns()));
        let result =
            initialized
                .card
                .step(&mut initialized.vehicle, &mut initialized.tail, &mut io);
        if stop.load(Ordering::Relaxed) {
            return Err(Error::Interrupted);
        }
        result?;
        if let Some(trace) = &mut frequency {
            let step_finished_ns = super::io::monotonic_ns();
            let thread_cpu_finished_ns = io.thread_cpu_ns();
            let (step_started_ns, thread_cpu_started_ns) =
                started.expect("enabled trace has clocks");
            trace.record(super::frequency_trace::TraceInput {
                state: io.subscribers(),
                controls_ready: initialized.card.settings.get_bool("ControlsReady")?,
                parser: initialized.vehicle.parser_readiness(),
                timing: super::frequency_trace::Timing {
                    step_started_ns,
                    step_finished_ns,
                    thread_cpu_started_ns,
                    thread_cpu_finished_ns,
                    receive_ns: initialized.card.diagnostics.receive,
                    stage_us: initialized.card.diagnostics.current,
                },
            })?;
        }
        monitor.monitor(monotonic);
        if let Some(trace) = &mut frequency {
            trace.completed_step(io.subscribers().frame())?;
        }
        if options
            .max_steps
            .is_some_and(|limit| monitor.frames >= limit.get())
        {
            return Ok(());
        }
    }
}
