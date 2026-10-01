//! Source cloudlog routing for native raylib callbacks, including input-worker logs.
use openpilot_logging::{
    producer::{Factory, Logger},
    record::{Level, Record},
};
use std::{cell::RefCell, io::Write};
thread_local! {static LOGGER:RefCell<Option<Logger>>=const {RefCell::new(None)};}
pub(crate) fn trace_log(level: i32, message: &str) {
    let (level, message) = match level {
        2 => (Level::Debug, format!("raylib: {message}")),
        3 => (Level::Info, format!("raylib: {message}")),
        4 => (Level::Warning, format!("raylib: {message}")),
        5 => (Level::Error, format!("raylib: {message}")),
        value => (
            Level::Error,
            format!("raylib: Unknown level {value}: {message}"),
        ),
    };
    emit(level, message);
}
pub fn emit(level: Level, message: String) {
    let result = LOGGER
        .try_with(|logger| {
            let mut logger = logger
                .try_borrow_mut()
                .map_err(|_| openpilot_logging::Error::Contract("recursive UI logging"))?;
            if logger.is_none() {
                *logger = Some(Factory::for_runtime()?.logger());
            }
            if let Some(logger) = logger.as_mut() {
                logger.emit(openpilot_logging::log_site!(), Record::text(level, message))?;
            }
            Ok::<_, openpilot_logging::Error>(())
        })
        .map_err(|_| openpilot_logging::Error::Contract("UI logger thread is closing"))
        .and_then(|result| result);
    if let Err(error) = result {
        // Logging failures must not unwind through the C callback or recursively log.
        match writeln!(std::io::stderr().lock(), "UI log transport: {error}") {
            Ok(()) | Err(_) => {}
        }
    }
}
