//! Managed child diagnostic boundary, called inside the translated daemon.
#![forbid(unsafe_code)]

use openpilot_crash_reporting::{Inputs, NativeException, Reporter, Sdk};
use openpilot_logging::{
    log_site,
    record::{Level, Record},
    Fields, Value,
};
use openpilot_logmessaged::JsonValue;
use std::{
    error::Error,
    ffi::CString,
    fmt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Prepare,
    Name,
    ResetContext,
    Tag,
    Body,
}

/// The concrete error remains available; its SDK description retains its actual Rust type.
#[derive(Debug)]
pub struct NativeError {
    pub exception: Box<NativeException>,
    source: Box<dyn Error>,
}
impl NativeError {
    pub fn new<E: Error + 'static>(error: E) -> Self {
        Self {
            exception: Box::new(NativeException::from_error(&error)),
            source: Box::new(error),
        }
    }
}
impl fmt::Display for NativeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.source.fmt(formatter)
    }
}
impl Error for NativeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}

#[derive(Debug)]
pub enum StepError {
    Interrupted,
    Raised(NativeError),
}
impl StepError {
    pub fn raised<E: Error + 'static>(error: E) -> Self {
        Self::Raised(NativeError::new(error))
    }
}
pub type Step<T> = Result<T, StepError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Returned,
    Interrupted,
}

#[derive(Debug, thiserror::Error)]
pub enum EntryError {
    #[error("{stage:?}: {error}")]
    Raised {
        stage: Stage,
        #[source]
        error: NativeError,
    },
    #[error("reporting {stage:?} error failed: {reporting}; original: {error}")]
    Reporting {
        stage: Stage,
        error: NativeError,
        #[source]
        reporting: openpilot_crash_reporting::Error,
    },
    #[error("logging managed interrupt failed: {0}")]
    InterruptLog(#[source] openpilot_logging::Error),
}

/// Cooperative SIGINT observation; the body explicitly returns `StepError::Interrupted`.
/// Dropping the handle unregisters only this registration. This does not unwind Rust code.
pub struct Sigint {
    requested: Arc<AtomicBool>,
    registration: signal_hook::SigId,
}
impl Sigint {
    pub fn install() -> std::io::Result<Self> {
        let requested = Arc::new(AtomicBool::new(false));
        let registration =
            signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&requested))?;
        Ok(Self {
            requested,
            registration,
        })
    }
    pub fn requested(&self) -> bool {
        self.requested.load(Ordering::Relaxed)
    }
}
impl Drop for Sigint {
    fn drop(&mut self) {
        signal_hook::low_level::unregister(self.registration);
    }
}

fn name(process: &str) -> Step<()> {
    let name = CString::new(process).map_err(StepError::raised)?;
    rustix::thread::set_name(&name).map_err(StepError::raised)
}

/// Execute the source launcher's ordering and diagnostic catch boundary in this process.
///
/// `prepare` replaces dynamic import. `reset_context` must create the fresh native IPC
/// ownership consumed by `body`; there is no Python global msgq context to reset here.
/// Call on the child main thread: Linux comm naming truncates to 15 bytes and does not
/// rewrite argv. Reporter initialization belongs to the caller's manager-init policy.
/// Panics and errors in other processes/threads are not intercepted by this API.
pub fn launch<S: Sdk, I: Inputs, P, C>(
    reporter: &mut Reporter<S, I>,
    process: &str,
    daemon: &str,
    prepare: impl FnOnce() -> Step<P>,
    reset_context: impl FnOnce(P) -> Step<C>,
    body: impl FnOnce(C, &mut Reporter<S, I>) -> Step<()>,
) -> Result<Outcome, EntryError> {
    let mut stage = Stage::Prepare;
    let result = (|| {
        let prepared = prepare()?;
        stage = Stage::Name;
        name(process)?;
        stage = Stage::ResetContext;
        let context = reset_context(prepared)?;
        let fields: Fields = [("daemon".into(), Value::Text(daemon.into()))]
            .into_iter()
            .collect();
        reporter.logger.bind(fields);
        stage = Stage::Tag;
        reporter
            .set_tag("daemon", &JsonValue::text(daemon))
            .map_err(StepError::raised)?;
        stage = Stage::Body;
        body(context, reporter)
    })();
    match result {
        Ok(()) => Ok(Outcome::Returned),
        Err(StepError::Interrupted) => {
            reporter
                .logger
                .emit(
                    log_site!(),
                    Record::text(Level::Warning, format!("child {process} got SIGINT")),
                )
                .map_err(EntryError::InterruptLog)?;
            Ok(Outcome::Interrupted)
        }
        Err(StepError::Raised(error)) => match reporter.capture_exception(&error.exception, true) {
            Ok(()) => Err(EntryError::Raised { stage, error }),
            Err(reporting) => Err(EntryError::Reporting {
                stage,
                error,
                reporting,
            }),
        },
    }
}
