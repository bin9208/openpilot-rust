use crate::{launch, EntryError, NativeError, Outcome, Step, StepError};
use openpilot_crash_reporting::{sdk::NativeSdk, Project, Reporter, RuntimeInputs};
use openpilot_logging::producer::Factory;
use std::{env, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Standalone(NativeError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error(transparent)]
    Reporting(#[from] openpilot_crash_reporting::Error),
    #[error(transparent)]
    Entry(#[from] EntryError),
}

/// Apply the inherited Python launcher boundary inside the translated child.
/// Standalone command-line tools retain their independent startup contract.
pub fn runtime(
    process: &str,
    daemon: &str,
    body: impl FnOnce() -> Step<()>,
) -> Result<Outcome, RuntimeError> {
    if env::var_os("MANAGER_DAEMON").is_none() {
        return match body() {
            Ok(()) => Ok(Outcome::Returned),
            Err(StepError::Interrupted) => Ok(Outcome::Interrupted),
            Err(StepError::Raised(error)) => Err(RuntimeError::Standalone(error)),
        };
    }
    let base = match env::var_os("OPENPILOT_BASEDIR") {
        Some(path) => PathBuf::from(path),
        None => env::current_dir()?,
    };
    let factory = Factory::for_runtime()?;
    let mut reporter = Reporter {
        sdk: NativeSdk::default(),
        inputs: RuntimeInputs::new(base),
        logger: factory.logger(),
    };
    reporter.init(Project::Selfdrive)?;
    Ok(launch(
        &mut reporter,
        process,
        daemon,
        || Ok(()),
        |()| Ok(()),
        |(), _| body(),
    )?)
}
