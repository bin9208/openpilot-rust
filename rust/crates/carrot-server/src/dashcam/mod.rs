//! Source: features/dashcam/{catalog,paths,read_state}.py; filesystem prerequisites only.
pub mod catalog;
pub mod paths;
pub mod read_state;
mod selectors;

use crate::Error;

#[derive(Debug, thiserror::Error)]
pub enum Failure {
    #[error("{message}")]
    Http { status: u16, message: String },
    #[error("invalid recent segment")]
    InvalidRecent,
    #[error(transparent)]
    Runtime(#[from] Error),
}
impl Failure {
    pub(super) fn http(status: u16, message: &str) -> Self {
        Self::Http {
            status,
            message: message.into(),
        }
    }
}
