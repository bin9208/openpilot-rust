//! Source: features/dashcam/{catalog,paths,read_state,routes}.py; catalogue/read-state HTTP subset.
mod cache;
pub mod catalog;
mod http;
mod pages;
pub mod paths;
pub mod read_state;
mod selectors;

pub use cache::Service;
pub use http::{handle, matches};

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
