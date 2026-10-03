use super::{mapping::Mapping, Buffer, CallResult, Device, Error};
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::os::fd::{FromRawFd, OwnedFd};

#[derive(Clone, Copy, Debug)]
pub struct AllocationOptions {
    pub length: usize,
    pub alignment: u32,
    pub flags: u32,
    pub mmu: [i32; 2],
}

pub struct Allocation<'device> {
    device: &'device Device,
    mapping: Option<Mapping>,
    handle: Option<u32>,
    length: usize,
}

impl Device {
    pub fn allocate(&self, options: AllocationOptions) -> Result<Allocation<'_>, Error> {
        if options.length == 0 || options.length > i32::MAX as usize || options.alignment == 0 {
            return Err(Error::AllocationSize);
        }
        let mut data = Buffer::<104>::zeroed();
        data.put64(0, options.length as u64);
        data.put64(8, u64::from(options.alignment));
        let mut count = 0;
        for (index, handle) in options.mmu.into_iter().enumerate() {
            if handle != 0 {
                data.put32(16 + index * 4, u32::from_le_bytes(handle.to_le_bytes()));
                count += 1;
            }
        }
        data.put32(80, count);
        data.put32(84, options.flags);
        // SAFETY: FFI: ALLOC_BUF reads/writes the verified 104-byte integer-only allocation ABI.
        let result = unsafe { self.camera(0x112, &mut data) };
        if result.code != 0 {
            eprintln!(
                "camera allocation ioctl: {} errno {}",
                result.code, result.errno
            );
        }
        let handle = data.get32(88);
        let fd = i32::from_le_bytes(data.get32(92).to_le_bytes());
        if fd <= 0 {
            if fd == 0 && (result.code == 0 || handle != 0) {
                // SAFETY: a successful allocation or returned handle identifies exported FD0.
                // An untouched failed ioctl has neither and must not close the process stdin.
                drop(unsafe { OwnedFd::from_raw_fd(fd) });
            }
            if handle != 0 {
                if let Err(cleanup) = self.release_buffer(handle).require_success(0x114) {
                    eprintln!("{cleanup}");
                }
            }
            return Err(Error::Allocation { fd, handle });
        }
        // SAFETY: FD ownership: the allocation ioctl exported this positive FD to this caller alone.
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        let mapping = match Mapping::new(fd, options.length) {
            Ok(mapping) => mapping,
            Err(error) => {
                if let Err(cleanup) = self.release_buffer(handle).require_success(0x114) {
                    eprintln!("{cleanup}");
                }
                return Err(error);
            }
        };
        Ok(Allocation {
            device: self,
            mapping: Some(mapping),
            handle: Some(handle),
            length: options.length,
        })
    }

    pub fn release_buffer(&self, handle: u32) -> CallResult {
        let mut data = Buffer::<8>::zeroed();
        data.put32(0, handle);
        // SAFETY: FFI: RELEASE_BUF consumes an eight-byte integer-only kernel-handle payload.
        unsafe { self.camera(0x114, &mut data) }
    }
}

impl Allocation<'_> {
    pub fn handle(&self) -> u32 {
        self.handle.unwrap_or(0)
    }
    pub fn len(&self) -> usize {
        self.length
    }
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }
    pub fn write(&mut self, offset: usize, bytes: &[u8]) -> Result<(), Error> {
        if let Some(mapping) = self.mapping.as_mut() {
            mapping.write(offset, bytes)
        } else {
            Err(Error::AllocationSize)
        }
    }
    pub fn zero(&mut self) {
        if let Some(mapping) = self.mapping.as_mut() {
            mapping.zero();
        }
    }
    pub fn close(mut self) -> Result<(), Error> {
        self.cleanup()
    }
    fn cleanup(&mut self) -> Result<(), Error> {
        drop(self.mapping.take());
        match self.handle.take() {
            Some(handle) => self.device.release_buffer(handle).require_success(0x114),
            None => Ok(()),
        }
    }
}

impl Drop for Allocation<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup() {
            eprintln!("{error}");
        }
    }
}

pub struct MemoryPool<'device> {
    device: &'device Device,
    cached: RefCell<BTreeMap<usize, VecDeque<Allocation<'device>>>>,
}

impl<'device> MemoryPool<'device> {
    pub fn new(device: &'device Device) -> Self {
        Self {
            device,
            cached: RefCell::new(BTreeMap::new()),
        }
    }
    pub fn lease(&self, length: usize) -> Result<PacketLease<'_, 'device>, Error> {
        let cached = self
            .cached
            .borrow_mut()
            .entry(length)
            .or_default()
            .pop_front();
        let mut allocation = match cached {
            Some(allocation) => allocation,
            None => self.device.allocate(AllocationOptions {
                length,
                alignment: 8,
                flags: 0x58,
                mmu: [0; 2],
            })?,
        };
        allocation.zero();
        Ok(PacketLease {
            pool: self,
            allocation: Some(allocation),
        })
    }
    pub fn close(&mut self) -> Result<(), Error> {
        for cache in self.cached.get_mut().values_mut() {
            while let Some(allocation) = cache.pop_front() {
                allocation.close()?;
            }
        }
        Ok(())
    }
}

pub struct PacketLease<'pool, 'device> {
    pool: &'pool MemoryPool<'device>,
    allocation: Option<Allocation<'device>>,
}

impl PacketLease<'_, '_> {
    pub fn handle(&self) -> u32 {
        self.allocation.as_ref().map_or(0, Allocation::handle)
    }
    pub fn write(&mut self, offset: usize, bytes: &[u8]) -> Result<(), Error> {
        match self.allocation.as_mut() {
            Some(allocation) => allocation.write(offset, bytes),
            None => Err(Error::AllocationSize),
        }
    }
}

impl Drop for PacketLease<'_, '_> {
    fn drop(&mut self) {
        if let Some(allocation) = self.allocation.take() {
            self.pool
                .cached
                .borrow_mut()
                .entry(allocation.length)
                .or_default()
                .push_back(allocation);
        }
    }
}
