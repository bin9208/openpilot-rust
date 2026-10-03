use super::{monotonic, transport_error, NativeIo};
use crate::{
    can_wire,
    core::{Error, StepIo},
    firmware_query::StartupIo,
    isotp,
    query::QueryIo,
};
use num_traits::ToPrimitive;
use openpilot_can::Frame;
use openpilot_logging::record::{Level, Record};
use std::{io, thread, time::Duration};

impl QueryIo for NativeIo {
    fn log(&mut self, level: crate::query::DiagnosticLevel, message: &str) {
        let level = match level {
            crate::query::DiagnosticLevel::Warning => Level::Warning,
            crate::query::DiagnosticLevel::Error | crate::query::DiagnosticLevel::Exception => {
                Level::Error
            }
        };
        let _ = self.carlog_text(level, message);
    }
    fn receive(&mut self, wait: bool) -> Result<Vec<Vec<Frame>>, isotp::Error> {
        self.drain_can(wait)
            .and_then(|messages| {
                messages
                    .iter()
                    .map(|bytes| Ok(can_wire::decode(bytes)?.frames))
                    .collect()
            })
            .map_err(transport_error)
    }
    fn send(&mut self, frames: &[Frame]) -> Result<(), isotp::Error> {
        let result = (|| {
            self.check_stop()?;
            let timestamp = (monotonic() * 1e9).to_u64().ok_or(Error::Numeric)?;
            let bytes = can_wire::sendcan(frames, true, timestamp)?;
            self.publisher.send("sendcan", &bytes)?;
            Ok(())
        })();
        result.map_err(transport_error)
    }
    fn sleep(&mut self, seconds: f64) -> Result<(), isotp::Error> {
        let duration = Duration::try_from_secs_f64(seconds).map_err(io::Error::other)?;
        thread::sleep(duration);
        self.check_stop().map_err(transport_error)
    }
    fn now(&mut self) -> f64 {
        monotonic()
    }
}
impl StartupIo for NativeIo {
    fn identification_event(&mut self, event: crate::identification::Event<'_>) {
        let result = (|| -> Result<(), crate::core::Error> {
            let console = event.console().map_err(io::Error::other)?;
            let value = serde_json::to_value(event).map_err(io::Error::other)?;
            let mut record = Record::text(Level::Error, console.clone());
            record.message = openpilot_logging::Value::from_json(value)?;
            self.emit_carlog(record, &console)?;
            Ok(())
        })();
        // Source logging transport failures do not abort fingerprinting.
        if let Err(_logging_error) = result {}
    }
    fn set_obd_multiplexing(&mut self, enabled: bool) -> Result<(), isotp::Error> {
        let result = (|| {
            if self.settings.get_bool("ObdMultiplexingEnabled")? == enabled {
                return Ok(());
            }
            self.warning(&format!(
                "Setting OBD multiplexing to {}",
                if enabled { "True" } else { "False" }
            ))?;
            self.settings.remove("ObdMultiplexingChanged")?;
            self.settings.put_bool("ObdMultiplexingEnabled", enabled)?;
            while self
                .settings
                .get("ObdMultiplexingChanged")?
                .is_none_or(|bytes| bytes.is_empty())
            {
                self.check_stop()?;
                thread::sleep(Duration::from_millis(10));
            }
            self.warning("OBD multiplexing set successfully")?;
            Ok(())
        })();
        result.map_err(transport_error)
    }
}
