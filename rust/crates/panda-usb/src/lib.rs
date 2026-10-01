#[cfg(feature = "native-skip-miri")]
mod api;
#[cfg(feature = "native-skip-miri")]
mod connection;
#[cfg(feature = "native-skip-miri")]
mod enumeration;
#[cfg(feature = "native-skip-miri")]
mod transfer;

#[cfg(feature = "native-skip-miri")]
pub use api::Api;
#[cfg(feature = "native-skip-miri")]
pub use connection::Session;
#[cfg(feature = "native-skip-miri")]
pub use enumeration::Enumerator;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[cfg(feature = "native-skip-miri")]
    #[error(transparent)]
    Library(#[from] libloading::Error),
    #[error("Error connecting to panda over USB")]
    Connection,
    #[error("Panda USB contract: {0}")]
    Contract(&'static str),
    #[error("Panda USB transfer mutex poisoned")]
    Poisoned,
}

#[derive(Debug)]
pub enum Log {
    Initialization,
    DeviceList,
    Issue {
        code: i32,
        description: String,
        operation: &'static str,
    },
    Disconnected,
    TransmitFull,
    Overflow(i32),
}

pub type Logger = Arc<dyn Fn(Log) + Send + Sync>;
