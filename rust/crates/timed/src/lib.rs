//! Source: openpilot/system/{timed,timezone_helper}.py and common/{time_helpers,gps}.py.
//! Native support daemon; production manager selection is deliberately separate.
pub mod clock;
pub mod runtime;
pub mod timezone;
pub mod wire;
use openpilot_logging::{
    log_site,
    producer::Logger,
    record::{Level, Record},
};
use std::{
    ffi::OsString,
    process::{Command, ExitStatus},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    StringParam(#[from] openpilot_params_typed::Error),
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[error(transparent)]
    State(#[from] openpilot_messaging::state::Error),
    #[error(transparent)]
    Capnp(#[from] capnp::Error),
    #[error("{0}")]
    Contract(&'static str),
}

pub trait Commands {
    /// A nonzero exit is handled by the source's CalledProcessError boundary;
    /// a failure to spawn escapes that boundary.
    fn run(&mut self, args: &[OsString]) -> Result<ExitStatus, std::io::Error>;
}
pub struct Sudo;
impl Commands for Sudo {
    fn run(&mut self, args: &[OsString]) -> Result<ExitStatus, std::io::Error> {
        Command::new("sudo").args(args).status()
    }
}
pub fn set_time(
    epoch: f64,
    clock: &dyn clock::Clock,
    services: &mut timezone::Services<'_>,
) -> Result<(), Error> {
    let diff = (clock.wall_seconds()? - epoch).abs();
    if diff < 10.0 {
        services.logger.emit(
            log_site!(),
            Record::text(Level::Debug, format!("Time diff too small: {diff:.1}s")),
        )?;
        return Ok(());
    }
    services.logger.emit(
        log_site!(),
        Record::text(
            Level::Info,
            format!("Setting system time from GPS (diff {diff:.1}s)"),
        ),
    )?;
    // Python int truncates toward zero; formatting a truncated finite value avoids a narrowing cast.
    let args = [
        OsString::from("date"),
        OsString::from("-s"),
        OsString::from(format!("@{:.0}", epoch.trunc())),
    ];
    let status = services.commands.run(&args)?;
    if !status.success() {
        exception(services.logger, "timed.failed_setting_time", &args, status)?;
    }
    Ok(())
}
fn exception(
    logger: &mut Logger,
    message: &str,
    args: &[OsString],
    status: ExitStatus,
) -> Result<(), Error> {
    logger.emit(
        log_site!(),
        Record::text(Level::Error, message.into()).with_exception(format!(
            "CalledProcessError: sudo {args:?} returned {status}"
        )),
    )?;
    Ok(())
}
