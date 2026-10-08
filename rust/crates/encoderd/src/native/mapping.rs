#![allow(unsafe_code)]
use crate::Error;
use std::{
    ffi::c_void,
    os::fd::{AsRawFd, OwnedFd},
    ptr::NonNull,
};

pub struct Mapping {
    pointer: NonNull<c_void>,
    length: usize,
    fd: OwnedFd,
}

// SAFETY: this owns its FD/mmap, exposes no Rust memory reference, and only
// supplies raw driver addresses or copies while the owner is retained.
unsafe impl Send for Mapping {}
unsafe impl Sync for Mapping {}

impl Mapping {
    /// # Safety
    /// The backing storage must cover length bytes and remain untruncated until
    /// this owner is dropped. Producer recycling is allowed and is not atomic.
    pub unsafe fn new(fd: OwnedFd, length: usize) -> Result<Self, Error> {
        if length == 0 {
            return Err(Error::Contract("empty DMA mapping"));
        }
        // SAFETY: the retained FD backs this shared mapping; length is the
        // validated imported size or the size just allocated by our allocator.
        let pointer = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                length,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd.as_raw_fd(),
                0,
            )
        };
        if pointer == libc::MAP_FAILED {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(Self {
            pointer: NonNull::new(pointer).ok_or(Error::Contract("null DMA mapping"))?,
            length,
            fd,
        })
    }
    pub fn address(&self) -> usize {
        self.pointer.as_ptr() as usize
    }
    pub fn length(&self) -> usize {
        self.length
    }
    pub fn fd(&self) -> &OwnedFd {
        &self.fd
    }
    pub fn byte(&self, offset: usize) -> Result<u8, Error> {
        if offset >= self.length {
            return Err(Error::Contract("DMA byte outside mapping"));
        }
        // SAFETY: the checked offset is live for this owner; a byte is copied
        // without creating a Rust reference to externally written memory.
        Ok(unsafe { self.pointer.as_ptr().cast::<u8>().add(offset).read() })
    }
    pub fn copy(&self, length: usize) -> Result<Vec<u8>, Error> {
        if length > self.length {
            return Err(Error::Contract("DMA copy exceeds mapping"));
        }
        let mut data = vec![0; length];
        // SAFETY: both ranges cover length bytes and are disjoint. External
        // producer writes may race this copy, just as in the original API.
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.pointer.as_ptr().cast::<u8>(),
                data.as_mut_ptr(),
                length,
            );
        }
        Ok(data)
    }
}
impl Drop for Mapping {
    fn drop(&mut self) {
        // SAFETY: this owner unmaps its exact region before its FD is closed.
        unsafe {
            libc::munmap(self.pointer.as_ptr(), self.length);
        }
    }
}
