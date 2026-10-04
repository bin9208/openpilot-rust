use crate::Error;
use openpilot_logging::record::Level;
use std::io::Write;

pub struct Carlog(Level);

impl Carlog {
    pub fn new() -> Result<Self, Error> {
        let text = std::env::var("LOGPRINT").map_or_else(
            |error| match error {
                std::env::VarError::NotPresent => Ok("INFO".to_owned()),
                std::env::VarError::NotUnicode(value) => {
                    Err(Error::LoggingLevel(value.to_string_lossy().into_owned()))
                }
            },
            Ok,
        )?;
        let level = match text.to_uppercase().as_str() {
            "NOTSET" => Level::NotSet,
            "DEBUG" => Level::Debug,
            "INFO" => Level::Info,
            "WARN" | "WARNING" => Level::Warning,
            "ERROR" => Level::Error,
            "FATAL" | "CRITICAL" => Level::Critical,
            _ => return Err(Error::LoggingLevel(text)),
        };
        Ok(Self(level))
    }
    pub fn warning(&self, text: &str) {
        if self.0 <= Level::Warning {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "source StreamHandler suppresses console write failures"
            )]
            let _ = writeln!(std::io::stderr().lock(), "{text}");
        }
    }
}
