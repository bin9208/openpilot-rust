use openpilot_journald::{child::Journal, decode, packet, parse_line, Error};
use openpilot_logging::{
    log_site,
    producer::Factory,
    record::{Level, Record},
    Fields, Value,
};
use openpilot_messaging::runtime::PubMaster;
use std::{
    process::ExitCode,
    sync::{atomic::AtomicBool, Arc},
};

#[derive(Debug, thiserror::Error)]
enum EntryError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Integer(#[from] std::num::TryFromIntError),
    #[error(transparent)]
    Journal(#[from] Error),
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error("monotonic timestamp overflow")]
    Timestamp,
}

fn monotonic_ns() -> Result<u64, EntryError> {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    u64::try_from(now.tv_sec)?
        .checked_mul(1_000_000_000)
        .and_then(|seconds| seconds.checked_add(u64::try_from(now.tv_nsec).ok()?))
        .ok_or(EntryError::Timestamp)
}

fn run() -> Result<(), EntryError> {
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let mut publisher = PubMaster::for_runtime(&["androidLog"])?;
    let factory = Factory::for_runtime()?;
    factory.bind_global(
        [
            ("daemon".into(), Value::Text("journald".into())),
            ("runtime_language".into(), Value::Text("rust".into())),
        ]
        .into_iter()
        .collect::<Fields>(),
    )?;
    let mut logger = factory.logger();
    let mut child = Journal::spawn()?;
    let result = (|| -> Result<(), EntryError> {
        while let Some(line) = child.next_line(&stop)? {
            match parse_line(&line) {
                Ok(Some(entry)) => {
                    publisher.send("androidLog", &packet(&entry, monotonic_ns()?))?
                }
                Ok(None) => {}
                Err(error @ Error::Json(decode::Error::Syntax { .. })) => {
                    logger.emit(
                        log_site!(),
                        Record::text(Level::Error, "failed to parse journalctl output".into())
                            .with_exception(error.to_string()),
                    )?;
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    })();
    child.terminate()?;
    result
}

fn main() -> ExitCode {
    match openpilot_managed_entry::runtime("openpilot.system.journald", "journald", || {
        run().map_err(openpilot_managed_entry::StepError::raised)
    }) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("journald: {error}");
            ExitCode::FAILURE
        }
    }
}
