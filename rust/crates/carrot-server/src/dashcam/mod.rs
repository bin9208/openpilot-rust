//! Source: features/dashcam/{catalog,paths,read_state,routes}.py; catalogue/read-state HTTP subset.
mod cache;
pub mod catalog;
mod file_routes;
mod http;
mod media;
mod media_http;
mod media_images;
mod media_video;
mod metadata;
mod mime;
mod pages;
pub mod paths;
mod raw_files;
pub mod read_state;
mod selectors;

pub use cache::Service;
pub use file_routes::{handle as metadata_handle, matches as metadata_matches, MetadataFiles};
pub use http::{handle, matches};
pub use media::Media;
pub use media_http::{handle as media_handle, matches as media_matches};

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
