use crate::Error;
use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ptr::{self, NonNull};

pub(crate) struct Buffer {
    ptr: NonNull<u8>,
    layout: Layout,
}

impl Buffer {
    pub fn new(bytes: usize) -> Result<Self, Error> {
        if bytes == 0 {
            return Err(Error::Allocation);
        }
        let layout = Layout::from_size_align(bytes, 4096).map_err(|_| Error::Allocation)?;
        // SAFETY: nonzero checked Layout; zero initialization permits byte reads.
        let ptr = NonNull::new(unsafe { alloc_zeroed(layout) }).ok_or(Error::Allocation)?;
        Ok(Self { ptr, layout })
    }

    pub fn view_ptr(&self, offset: usize, bytes: usize) -> Result<*mut u8, Error> {
        if offset
            .checked_add(bytes)
            .is_none_or(|end| end > self.layout.size())
        {
            return Err(Error::Invalid {
                kind: "buffer range",
                index: offset,
            });
        }
        // SAFETY: checked range is inside this owned allocation, retaining provenance.
        Ok(unsafe { self.ptr.as_ptr().add(offset) })
    }

    pub fn write(&mut self, offset: usize, source: &[u8]) -> Result<(), Error> {
        let destination = self.view_ptr(offset, source.len())?;
        // SAFETY: checked destination; safe callers cannot borrow this private storage.
        unsafe { ptr::copy_nonoverlapping(source.as_ptr(), destination, source.len()) };
        Ok(())
    }

    pub fn read(&self, offset: usize, destination: &mut [u8]) -> Result<(), Error> {
        let source = self.view_ptr(offset, destination.len())?;
        // SAFETY: initialized checked source; destination cannot alias private storage.
        unsafe { ptr::copy_nonoverlapping(source, destination.as_mut_ptr(), destination.len()) };
        Ok(())
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        // SAFETY: this owner frees its allocation once with the original Layout.
        unsafe { dealloc(self.ptr.as_ptr(), self.layout) };
    }
}

#[cfg(test)]
mod tests {
    use super::Buffer;

    #[test]
    fn cached_views_survive_writes_reads_and_owner_moves() {
        let mut buffer = Buffer::new(32).unwrap();
        let pointer = buffer.view_ptr(8, 8).unwrap().cast::<u64>();
        buffer.write(0, &[1; 32]).unwrap();
        let mut moved = buffer;
        // SAFETY: offset 8 is u64-aligned in the live 4096-aligned 32-byte allocation.
        unsafe { pointer.write(42) };
        let mut output = [0; 8];
        moved.read(8, &mut output).unwrap();
        assert_eq!(u64::from_ne_bytes(output), 42);
        moved.write(8, &99_u64.to_ne_bytes()).unwrap();
        // SAFETY: the cached pointer retains its provenance across copy operations.
        assert_eq!(unsafe { pointer.read() }, 99);
    }

    #[test]
    fn zeroes_storage_and_rejects_overflow_and_out_of_bounds() {
        let mut buffer = Buffer::new(16).unwrap();
        let mut output = [1; 16];
        buffer.read(0, &mut output).unwrap();
        assert_eq!(output, [0; 16]);
        assert!(buffer.write(15, &[1, 2]).is_err());
        assert!(buffer.read(usize::MAX, &mut output).is_err());
        assert!(Buffer::new(0).is_err());
        assert!(Buffer::new(usize::MAX).is_err());
    }
}
