use super::{clock_ns, monotonic, monotonic_ns, NativeIo};
use crate::core::{Error, StepIo};
use openpilot_logging::{
    log_site,
    record::{Level, Record},
    Fields, Number,
};
use openpilot_messaging::state::State;
use std::time::Duration;

impl StepIo for NativeIo {
    fn settings_flags(&self) -> Option<crate::core::SettingsFlags> {
        self.params_reader
            .as_ref()
            .map(super::super::params_poll::ParamsPoller::flags)
    }
    fn vehicle_log(&mut self, diagnostic: &crate::core::VehicleLog) -> Result<(), Error> {
        let level = match diagnostic.level {
            crate::query::DiagnosticLevel::Warning => Level::Warning,
            crate::query::DiagnosticLevel::Error | crate::query::DiagnosticLevel::Exception => {
                Level::Error
            }
        };
        self.carlog_text(level, &diagnostic.message)
    }
    fn put_nonblocking(&mut self, key: &str, bytes: &[u8]) -> Result<(), Error> {
        self.check_stop()?;
        self.writes.put(key, bytes)
    }
    fn receive_can_raw(&mut self) -> Result<Vec<Vec<u8>>, Error> {
        self.drain_can(true)
    }
    fn update_subscribers(&mut self) -> Result<(), Error> {
        let messages: Vec<_> = self
            .subscriptions
            .receive(Duration::ZERO)?
            .into_iter()
            .map(|message| message.bytes)
            .collect();
        Ok(self.state.update_with_clock(&messages, monotonic)?)
    }
    fn subscribers(&self) -> &State {
        &self.state
    }
    fn monotonic_ns(&mut self) -> u64 {
        monotonic_ns()
    }
    fn thread_cpu_ns(&mut self) -> u64 {
        clock_ns(rustix::time::ClockId::ThreadCPUTime)
    }
    fn publish(&mut self, topic: &str, bytes: &[u8]) -> Result<(), Error> {
        self.check_stop()?;
        Ok(self.publisher.send(topic, bytes)?)
    }
    fn warning(&mut self, message: &str) -> Result<(), Error> {
        self.logger.emit(
            log_site!(),
            Record::text(Level::Warning, message.to_owned()),
        )?;
        Ok(())
    }
    fn car_warning(&mut self, message: &str) -> Result<(), Error> {
        self.carlog_text(Level::Warning, message)
    }
    fn diagnostics(&mut self, values: &[(&'static str, f64)]) -> Result<(), Error> {
        self.diagnostics.record(
            &mut self.logger,
            log_site!(),
            values
                .iter()
                .map(|(key, value)| (key.to_string(), Number::Float(*value))),
            Fields::new(),
        )?;
        Ok(())
    }
}
