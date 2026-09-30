//! Project-owned Sentry policy. The transport/SDK is a native external dependency.
#![forbid(unsafe_code)]
mod normalize;
pub mod policy;
pub mod runtime;
pub mod sdk;
pub use policy::{Configuration, Inputs, Project, Reporter, Sdk};
pub use runtime::{ParamsSource, RuntimeInputs};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error(transparent)]
    Metadata(#[from] openpilot_runtime_version::Error),
    #[error(transparent)]
    Json(#[from] openpilot_logmessaged::JsonError),
    #[error(transparent)]
    Encode(#[from] serde_json::Error),
    #[error(transparent)]
    Format(#[from] std::fmt::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    ParamsString(#[from] openpilot_params_typed::Error),
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error("non-Unicode {0}")]
    Unicode(&'static str),
    #[error("SDK {operation}: {detail}")]
    Sdk {
        operation: &'static str,
        detail: String,
    },
}
/// Explicit native exception data, without pretending Rust has a Python traceback.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NativeException {
    pub kind: String,
    pub message: String,
    #[serde(default)]
    pub causes: Vec<String>,
    #[serde(default)]
    pub backtrace: Option<String>,
}
impl NativeException {
    pub fn from_error<E: std::error::Error + 'static>(error: &E) -> Self {
        let mut causes = Vec::new();
        let mut source = error.source();
        while let Some(error) = source {
            causes.push(error.to_string());
            source = error.source();
        }
        Self {
            kind: std::any::type_name::<E>().into(),
            message: error.to_string(),
            causes,
            backtrace: Some(std::backtrace::Backtrace::capture().to_string()),
        }
    }
    pub fn diagnostic(&self) -> String {
        let mut text = format!("{}: {}\n", self.kind, self.message);
        for cause in &self.causes {
            text.push_str(&format!("caused by: {cause}\n"));
        }
        if let Some(trace) = &self.backtrace {
            text.push_str(trace);
            if !trace.ends_with('\n') {
                text.push('\n');
            }
        }
        text
    }
}
