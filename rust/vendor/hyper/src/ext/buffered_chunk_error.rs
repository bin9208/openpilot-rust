use bytes::Bytes;
use std::{error::Error, fmt, io};

#[derive(Debug)]
/// Buffered chunk-start failure with its raw line and unchanged native cause.
pub struct BufferedChunkStartError {
    line: Bytes,
    cause: io::Error,
}

impl BufferedChunkStartError {
    pub(crate) fn new(line: Bytes, cause: io::Error) -> Self {
        Self { line, cause }
    }

    /// Raw line captured only after rejection, stopping at the buffered CRLF.
    pub fn line(&self) -> &[u8] {
        &self.line
    }
}

impl fmt::Display for BufferedChunkStartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.cause, formatter)
    }
}

impl Error for BufferedChunkStartError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.cause)
    }
}
