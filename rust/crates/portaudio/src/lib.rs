#[cfg(feature = "native-skip-miri")]
mod blocking;
#[cfg(any(feature = "native-skip-miri", test))]
mod callback;
#[cfg(feature = "native-skip-miri")]
mod native;
#[cfg(feature = "native-skip-miri")]
pub use blocking::play as play_blocking;
#[cfg(feature = "native-skip-miri")]
pub use native::Stream;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("PortAudio {operation}: error {code}")]
    Api { operation: &'static str, code: i32 },
    #[cfg(feature = "native-skip-miri")]
    #[error(transparent)]
    Library(#[from] libloading::Error),
    #[error("PortAudio contract: {0}")]
    Contract(&'static str),
}
