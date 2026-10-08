//! Port of openpilot/selfdrive/carrot/server/features/xiaoge.py; inference remains external.
mod http;
mod online;

pub use http::{handle, matches};
pub use online::Online;

const MAX_REQUEST_BYTES: usize = 65536;
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
enum ProxyError {
    #[error("vision_unavailable")]
    Unavailable,
    #[error("vision_timeout")]
    Timeout,
    #[error("vision_bad_response")]
    BadResponse,
}
