use super::{abi, io, Device, Queue};
use crate::Error;
use std::{
    os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd},
    sync::Arc,
};

pub struct Allocator {
    fd: Arc<OwnedFd>,
}
pub struct Buffer {
    inner: io::IonBuffer,
}
pub struct QueuedPlane {
    pub index: u32,
    pub used: usize,
    pub timestamp: u64,
    pub timestamp_copy: bool,
}
impl Allocator {
    /// # Errors
    /// Preserves failure to open the native ION device.
    pub fn open() -> Result<Self, Error> {
        Ok(Self {
            fd: Arc::new(io::open("/dev/ion")?),
        })
    }
    /// # Errors
    /// Preserves ION allocation/share/mapping errors and size overflow.
    pub fn allocate(&self, length: usize) -> Result<Buffer, Error> {
        Ok(Buffer {
            inner: io::IonBuffer::allocate(Arc::clone(&self.fd), length)?,
        })
    }
}
impl Buffer {
    pub fn with_input<R>(&mut self, callback: impl FnOnce(&mut [u8], BorrowedFd<'_>) -> R) -> R {
        // SAFETY: the exclusively borrowed mapping is bounded by this retained
        // allocation; its FD and byte view are scoped to the synchronous callback.
        callback(
            unsafe {
                std::slice::from_raw_parts_mut(
                    self.inner.mapping().address() as *mut u8,
                    self.inner.length(),
                )
            },
            self.as_fd(),
        )
    }
    pub fn with_bytes_mut<R>(&mut self, callback: impl FnOnce(&mut [u8]) -> R) -> R {
        // SAFETY: the exclusive owner supplies one bounded view of its live
        // mapping for this synchronous callback; no slice escapes its lifetime.
        callback(unsafe {
            std::slice::from_raw_parts_mut(
                self.inner.mapping().address() as *mut u8,
                self.inner.length(),
            )
        })
    }
    /// # Errors
    /// Rejects reads outside the retained allocation.
    pub fn copy_out(&self, length: usize) -> Result<Vec<u8>, Error> {
        if length > self.length() {
            return Err(Error::Contract("DMA read exceeds allocation"));
        }
        // SAFETY: the checked range is inside this owner's live mapping and is
        // copied synchronously, retaining no references to external memory.
        Ok(unsafe {
            std::slice::from_raw_parts(self.inner.mapping().address() as *const u8, length)
        }
        .to_vec())
    }
    /// # Errors
    /// Preserves the ION cache-clean error before queueing this input.
    pub fn sync_to_device(&self) -> Result<(), Error> {
        self.inner.sync_to_device()
    }
    #[must_use]
    pub fn length(&self) -> usize {
        self.inner.length()
    }
    /// # Errors
    /// Preserves failure to duplicate the retained DMA-BUF file descriptor.
    pub fn duplicate_fd(&self) -> Result<OwnedFd, Error> {
        Ok(self.inner.mapping().fd().try_clone()?)
    }
    /// # Errors
    /// Rejects writes outside this allocation and preserves cache-sync errors.
    pub fn write(&mut self, bytes: &[u8]) -> Result<(), Error> {
        if bytes.len() > self.inner.length() {
            return Err(Error::Contract("DMA write exceeds allocation"));
        }
        // SAFETY: the checked destination covers bytes.len(); no Rust reference
        // aliases the external mapping, and the source slice is disjoint.
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                self.inner.mapping().address() as *mut u8,
                bytes.len(),
            );
        }
        self.inner.sync_to_device()
    }
    /// # Errors
    /// Preserves the ION invalidation/cache-sync error.
    pub fn sync_from_device(&self) -> Result<(), Error> {
        self.inner.sync_from_device()
    }
}
impl AsFd for Buffer {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.inner.mapping().fd().as_fd()
    }
}
impl Device {
    /// # Errors
    /// Rejects an oversized plane and preserves its QBUF kernel error.
    pub fn queue(&self, queue: Queue, buffer: &Buffer, plane: QueuedPlane) -> Result<(), Error> {
        if plane.used > buffer.length() {
            return Err(Error::Contract("V4L plane exceeds retained allocation"));
        }
        let mut native = abi::v4l2_plane {
            bytesused: u32::try_from(plane.used)?,
            length: u32::try_from(buffer.length())?,
            ..Default::default()
        };
        native.m.userptr = buffer.inner.mapping().address().try_into()?;
        native.reserved[0] = u32::try_from(buffer.inner.mapping().fd().as_raw_fd())?;
        let mut value = abi::v4l2_buffer {
            index: plane.index,
            type_: queue.kind(),
            flags: if plane.timestamp_copy {
                abi::V4L2_BUF_FLAG_TIMESTAMP_COPY
            } else {
                0
            },
            memory: abi::V4L2_MEMORY_USERPTR,
            length: 1,
            ..Default::default()
        };
        value.timestamp.tv_sec = (plane.timestamp / 1_000_000).try_into()?;
        value.timestamp.tv_usec = (plane.timestamp % 1_000_000).try_into()?;
        value.m.planes = &mut native;
        // SAFETY: the nested plane and owned mapping stay live for QBUF; the
        // Rust codec policy retains the Buffer until its dequeue or stream stop.
        unsafe { io::ioctl(&self.fd, abi::ENCODER_VIDIOC_QBUF, &mut value) }
    }
}
