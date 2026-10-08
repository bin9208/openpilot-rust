//! Per-render CPU and elapsed timing from selfdrive/ui/render_diagnostics.py.
use crate::Error;
use num_traits::ToPrimitive;
use openpilot_logging::{
    producer::{Factory, Logger},
    runtime::RuntimeDiagnostics,
    Fields, Number,
};

pub trait Clock {
    fn monotonic_ns(&self) -> i128;
    fn thread_ns(&self) -> i128;
}
pub struct SystemClock;
fn nanoseconds(clock: rustix::time::ClockId) -> i128 {
    let time = rustix::time::clock_gettime(clock);
    i128::from(time.tv_sec) * 1_000_000_000 + i128::from(time.tv_nsec)
}
impl Clock for SystemClock {
    fn monotonic_ns(&self) -> i128 {
        nanoseconds(rustix::time::ClockId::Monotonic)
    }
    fn thread_ns(&self) -> i128 {
        nanoseconds(rustix::time::ClockId::ThreadCPUTime)
    }
}
fn milliseconds(ns: i128) -> Result<f64, Error> {
    Ok(ns
        .to_f64()
        .ok_or(Error::Contract("render timing magnitude"))?
        * 1e-6)
}
pub struct Timings<C: Clock> {
    clock: C,
    started: i128,
    cpu_started: i128,
    values: Vec<(String, f64)>,
}
impl<C: Clock> Timings<C> {
    pub fn new(clock: C) -> Self {
        Self {
            clock,
            started: 0,
            cpu_started: 0,
            values: Vec::new(),
        }
    }
    pub fn start(&mut self) {
        self.values.clear();
        self.started = self.clock.monotonic_ns();
        self.cpu_started = self.clock.thread_ns();
    }
    pub fn call<T>(
        &mut self,
        name: &str,
        callback: impl FnOnce() -> Result<T, Error>,
    ) -> Result<T, Error> {
        let started = self.clock.monotonic_ns();
        let cpu_started = self.clock.thread_ns();
        let result = callback();
        let cpu = milliseconds(self.clock.thread_ns() - cpu_started)?;
        let elapsed = milliseconds(self.clock.monotonic_ns() - started)?;
        for (suffix, value) in [("_ms", elapsed), ("_cpu_ms", cpu)] {
            let key = format!("{name}{suffix}");
            if let Some((_, total)) = self.values.iter_mut().find(|(k, _)| k == &key) {
                *total += value;
            } else {
                self.values.push((key, value));
            }
        }
        result
    }
    pub fn finish(&self) -> Result<Vec<(String, Number)>, Error> {
        let elapsed = milliseconds(self.clock.monotonic_ns() - self.started)?;
        let cpu = milliseconds(self.clock.thread_ns() - self.cpu_started)?;
        Ok([
            ("work_ms".into(), Number::Float(elapsed)),
            ("thread_cpu_ms".into(), Number::Float(cpu)),
        ]
        .into_iter()
        .chain(
            self.values
                .iter()
                .map(|(k, v)| (k.clone(), Number::Float(*v))),
        )
        .collect())
    }
}
pub struct RenderDiagnostics {
    timings: Timings<SystemClock>,
    runtime: RuntimeDiagnostics,
    logger: Logger,
}
impl RenderDiagnostics {
    pub fn new(component: &str) -> Result<Self, Error> {
        Ok(Self {
            timings: Timings::new(SystemClock),
            runtime: RuntimeDiagnostics::new(component, 1.),
            logger: Factory::for_runtime()?.logger(),
        })
    }
    pub fn start(&mut self) {
        self.timings.start();
    }
    pub fn call<T>(
        &mut self,
        name: &str,
        callback: impl FnOnce() -> Result<T, Error>,
    ) -> Result<T, Error> {
        self.timings.call(name, callback)
    }
    pub fn finish(&mut self) -> Result<(), Error> {
        self.runtime.record(
            &mut self.logger,
            openpilot_logging::log_site!(),
            self.timings.finish()?,
            Fields::new(),
        )?;
        Ok(())
    }
}
