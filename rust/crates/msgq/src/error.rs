use std::{collections::TryReserveError, io};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("native msgq: {0}: {1}")]
    Io(&'static str, #[source] io::Error),
    #[error("native msgq: {0}")]
    Invalid(&'static str),
    #[error("native msgq: corrupt shared-memory contract: {0}")]
    Corrupt(&'static str),
    #[error("native msgq: another publisher owns this endpoint")]
    PublisherReplaced,
    #[error("native msgq: allocation failed: {0}")]
    Allocation(#[from] TryReserveError),
    #[error("timeout exceeds the native millisecond range")]
    TimeoutRange,
}

impl Error {
    #[cfg(feature = "native-skip-miri")]
    pub(crate) fn last(operation: &'static str) -> Self {
        Self::Io(operation, io::Error::last_os_error())
    }
}
