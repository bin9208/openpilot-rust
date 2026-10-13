use super::{Buffer, Error, VisionClient, VisionLayout};
use std::os::fd::{AsFd, BorrowedFd};

/// Imported camera memory remains owned by the client. The producer may recycle it.
pub struct RetainedBuffer<'client> {
    buffer: &'client Buffer,
}
impl VisionClient {
    /// Access a previously imported buffer without receiving or reconnecting.
    /// The client borrow keeps its mapping/FD alive; this is not a producer lease.
    pub fn retained_buffer(&self, index: usize) -> Result<RetainedBuffer<'_>, Error> {
        Ok(RetainedBuffer {
            buffer: self
                .buffers
                .get(index)
                .ok_or(Error::Invalid("retained camera index is out of range"))?,
        })
    }
}
impl RetainedBuffer<'_> {
    pub fn layout(&self) -> VisionLayout {
        self.buffer.wire.layout
    }
    pub fn copy_into(&self, destination: &mut [u8]) -> Result<(), Error> {
        self.buffer.mapping.copy_into(destination)
    }
}
impl AsFd for RetainedBuffer<'_> {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.buffer.fd.as_fd()
    }
}
