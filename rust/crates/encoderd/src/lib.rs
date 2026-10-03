pub mod config;
pub mod lifecycle;
#[cfg(feature = "native")]
pub mod native;
pub mod profile;
pub mod sync;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("encoder source contract: {0}")]
    Contract(&'static str),
    #[error(transparent)]
    Integer(#[from] std::num::TryFromIntError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[cfg(feature = "native")]
    #[error(transparent)]
    Msgq(#[from] openpilot_msgq::Error),
    #[cfg(feature = "native")]
    #[error(transparent)]
    Capnp(#[from] capnp::Error),
    #[cfg(feature = "native")]
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error("FFmpeg {operation} failed: {code}")]
    Ffmpeg { operation: &'static str, code: i32 },
}
