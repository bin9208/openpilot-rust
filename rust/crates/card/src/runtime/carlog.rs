use super::NativeIo;
use crate::core::Error;
use openpilot_logging::{
    log_site,
    record::{Level, Record},
};
use std::io::Write;

pub(super) fn level(value: Option<&str>) -> Result<Level, Error> {
    match value.unwrap_or("INFO").to_uppercase().as_str() {
        "NOTSET" => Ok(Level::NotSet),
        "DEBUG" => Ok(Level::Debug),
        "INFO" => Ok(Level::Info),
        "WARN" | "WARNING" => Ok(Level::Warning),
        "ERROR" => Ok(Level::Error),
        "FATAL" | "CRITICAL" => Ok(Level::Critical),
        _ => Err(Error::LoggingLevel(value.unwrap_or("INFO").to_owned())),
    }
}

impl NativeIo {
    pub(super) fn emit_carlog(&mut self, record: Record, console: &str) -> Result<(), Error> {
        if record.level < self.carlog_level {
            return Ok(());
        }
        let _ = writeln!(std::io::stderr().lock(), "{console}");
        self.logger.emit(log_site!(), record)?;
        Ok(())
    }
    pub(super) fn carlog_text(&mut self, level: Level, message: &str) -> Result<(), Error> {
        self.emit_carlog(Record::text(level, message.to_owned()), message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_carlog_level_uses_uppercase_with_python_aliases() {
        for (input, expected) in [
            (None, Level::Info),
            (Some("dEbUg"), Level::Debug),
            (Some("WARN"), Level::Warning),
            (Some("fatal"), Level::Critical),
            (Some("ERROR"), Level::Error),
            (Some("NOTSET"), Level::NotSet),
        ] {
            assert_eq!(level(input).unwrap(), expected);
        }
        for input in ["", "10", " INFO ", "unknown"] {
            assert!(level(Some(input)).is_err());
        }
    }
}
