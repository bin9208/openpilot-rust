use super::Error;
use std::os::fd::{AsRawFd, OwnedFd};
use std::ptr::NonNull;

pub(super) struct Mapping {
    pointer: NonNull<u8>,
    length: usize,
    _fd: OwnedFd,
}

impl Mapping {
    pub(super) fn new(fd: OwnedFd, length: usize) -> Result<Self, Error> {
        if length == 0 || length > isize::MAX as usize {
            return Err(Error::AllocationSize);
        }
        // SAFETY: FFI: mmap receives an owned live FD, nonzero length, and no requested address.
        let raw = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                length,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd.as_raw_fd(),
                0,
            )
        };
        if raw == libc::MAP_FAILED {
            return Err(Error::Mapping(std::io::Error::last_os_error()));
        }
        let Some(pointer) = NonNull::new(raw.cast::<u8>()) else {
            // SAFETY: ownership: mmap succeeded at address zero; release this unusable Rust mapping.
            let result = unsafe { libc::munmap(raw, length) };
            if result != 0 {
                eprintln!(
                    "camera null mapping cleanup: {}",
                    std::io::Error::last_os_error()
                );
            }
            return Err(Error::Mapping(std::io::Error::from_raw_os_error(
                libc::ENOMEM,
            )));
        };
        Ok(Self {
            pointer,
            length,
            _fd: fd,
        })
    }

    pub(super) fn write(&mut self, offset: usize, data: &[u8]) -> Result<(), Error> {
        if offset
            .checked_add(data.len())
            .is_none_or(|end| end > self.length)
        {
            return Err(Error::MemoryBounds {
                offset,
                length: data.len(),
                capacity: self.length,
            });
        }
        // SAFETY: bounds: the checked range belongs to the live mapping; no mapped references escape.
        let destination = unsafe { self.pointer.as_ptr().add(offset) };
        // SAFETY: aliasing: safe callers cannot borrow this private mapping as their source slice.
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), destination, data.len()) };
        Ok(())
    }

    pub(super) fn zero(&mut self) {
        // SAFETY: bounds/initialization: mmap owns this writable length and no mapped references escape.
        unsafe { std::ptr::write_bytes(self.pointer.as_ptr(), 0, self.length) };
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        // SAFETY: ownership: this object is the sole owner of exactly this mmap address and length.
        let result = unsafe { libc::munmap(self.pointer.as_ptr().cast(), self.length) };
        if result != 0 {
            eprintln!("camera munmap: {}", std::io::Error::last_os_error());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mapping_rejects_out_of_bounds_writes() {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/zero")
            .unwrap();
        let mut mapping = Mapping::new(file.into(), 4096).unwrap();
        mapping.zero();
        mapping.write(4092, &[1, 2, 3, 4]).unwrap();
        assert!(matches!(
            mapping.write(4093, &[1, 2, 3, 4]),
            Err(Error::MemoryBounds { .. })
        ));
        assert!(matches!(
            mapping.write(usize::MAX, &[1]),
            Err(Error::MemoryBounds { .. })
        ));
    }
}
