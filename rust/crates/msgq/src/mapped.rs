use crate::{memory, queue_core::QueueMemory, Error};
use std::{os::fd::AsRawFd, os::fd::BorrowedFd, ptr::NonNull};

struct Region {
    base: NonNull<u8>,
    length: usize,
}

impl Region {
    fn new(fd: BorrowedFd<'_>, length: usize, populate: bool) -> Result<Self, Error> {
        if length == 0 || length > isize::MAX as usize {
            return Err(Error::Invalid("invalid mapping length"));
        }
        let flags = libc::MAP_SHARED | if populate { libc::MAP_POPULATE } else { 0 };
        // SAFETY: the borrowed FD remains open during mmap; no address hint is
        // supplied, and the kernel validates the requested shared writable mapping.
        let pointer = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                length,
                libc::PROT_READ | libc::PROT_WRITE,
                flags,
                fd.as_raw_fd(),
                0,
            )
        };
        if pointer == libc::MAP_FAILED {
            return Err(Error::last("map shared memory"));
        }
        let Some(base) = NonNull::new(pointer.cast::<u8>()) else {
            // SAFETY: mmap succeeded at address zero; that live allocation must be
            // released because Rust references and NonNull cannot represent it.
            if unsafe { libc::munmap(pointer, length) } != 0 {
                eprintln!("{}", Error::last("release zero-address mapping"));
            }
            return Err(Error::Invalid("zero-address mapping is unsupported"));
        };
        Ok(Self { base, length })
    }
}

impl Drop for Region {
    fn drop(&mut self) {
        // SAFETY: this owner obtained exactly this live region from mmap and no
        // borrowed memory view can outlive its shared borrow of the owner.
        if unsafe { libc::munmap(self.base.as_ptr().cast(), self.length) } != 0 {
            eprintln!("{}", Error::last("unmap shared memory"));
        }
    }
}

pub(crate) struct WordMapping(Region);

impl WordMapping {
    pub(crate) fn new(fd: BorrowedFd<'_>, length: usize, populate: bool) -> Result<Self, Error> {
        if !length.is_multiple_of(8) {
            return Err(Error::Invalid("queue mapping is not word aligned"));
        }
        Ok(Self(Region::new(fd, length, populate)?))
    }

    pub(crate) fn memory(&self) -> Result<QueueMemory<'_>, Error> {
        // SAFETY: mmap supplies a page-aligned initialized region owned by self.
        // This mapping exposes only 64-bit atomic access, tied to this borrow.
        let words = unsafe { memory::queue_words(self.0.base, self.0.length) }?;
        QueueMemory::new(words)
    }
}

pub(crate) struct ImageMapping {
    region: Region,
    data_length: usize,
}

impl ImageMapping {
    pub(crate) fn new(
        fd: BorrowedFd<'_>,
        data_length: usize,
        mapped_length: usize,
    ) -> Result<Self, Error> {
        if data_length == 0
            || data_length
                .checked_add(8)
                .is_none_or(|end| end > mapped_length)
        {
            return Err(Error::Invalid("invalid image mapping length"));
        }
        Ok(Self {
            region: Region::new(fd, mapped_length, false)?,
            data_length,
        })
    }

    fn memory(&self) -> Result<memory::ImageMemory<'_>, Error> {
        // SAFETY: construction validates pixels plus the frame-ID tail, mmap owns
        // initialized shared storage, and this API uses only ImageMemory's atomics.
        unsafe { memory::ImageMemory::new(self.region.base, self.data_length, self.region.length) }
    }

    pub(crate) fn address(&self) -> usize {
        self.region.base.as_ptr().expose_provenance()
    }

    pub(crate) fn mapped_length(&self) -> usize {
        self.region.length
    }

    pub(crate) fn write(&self, offset: usize, bytes: &[u8]) -> Result<(), Error> {
        self.memory()?.write(offset, bytes)
    }

    pub(crate) fn copy_into(&self, destination: &mut [u8]) -> Result<(), Error> {
        self.memory()?.copy_into(destination)
    }

    pub(crate) fn frame_id(&self) -> Result<u64, Error> {
        Ok(self.memory()?.frame_id())
    }

    pub(crate) fn set_frame_id(&self, value: u64) -> Result<(), Error> {
        self.memory()?.set_frame_id(value);
        Ok(())
    }
}
