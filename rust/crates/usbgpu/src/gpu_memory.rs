use crate::{
    asic::Asic,
    memory::Mapping,
    page_table::AddressSpace,
    runtime_bus::{CpuLocation, RuntimeBus},
    Error,
};
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug)]
pub struct Buffer {
    id: u64,
    offset: u64,
    size: u64,
    address: u64,
}
impl Buffer {
    pub fn address(self) -> u64 {
        self.address
    }
    pub fn size(self) -> u64 {
        self.size
    }
    pub fn view(self, offset: u64, size: u64) -> Result<Self, Error> {
        if offset.checked_add(size).is_none_or(|end| end > self.size) {
            return Err(Error::Contract("GPU buffer view outside allocation"));
        }
        Ok(Self {
            id: self.id,
            offset: self.offset + offset,
            size,
            address: self.address + offset,
        })
    }
}
struct Allocation {
    mapping: Mapping,
    cpu: Option<CpuLocation>,
}
#[derive(Clone, Copy, Default)]
pub struct BufferOptions {
    pub host: bool,
    pub uncached: bool,
    pub cpu_access: bool,
}
pub struct SystemPool {
    pub buffer: Buffer,
    pub next: u64,
}
pub struct Heap<B: RuntimeBus> {
    pub asic: Asic<B>,
    allocations: BTreeMap<u64, Allocation>,
    next_id: u64,
}
impl<B: RuntimeBus> Heap<B> {
    pub fn new(asic: Asic<B>) -> Self {
        Self {
            asic,
            allocations: BTreeMap::new(),
            next_id: 0,
        }
    }
    fn insert(&mut self, mapping: Mapping, cpu: Option<CpuLocation>) -> Result<Buffer, Error> {
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(Error::Contract("GPU buffer id exhausted"))?;
        let result = Buffer {
            id,
            offset: 0,
            size: mapping.size,
            address: mapping.address,
        };
        self.allocations.insert(id, Allocation { mapping, cpu });
        Ok(result)
    }
    pub fn map_system(
        &mut self,
        controller: u64,
        physical: u64,
        size: u64,
    ) -> Result<Buffer, Error> {
        let address = self.asic.memory.alloc_virtual(size, 4096)?;
        let mapping = self.asic.memory.map(
            &mut self.asic.hw,
            Mapping {
                address,
                size,
                physical: vec![(physical, size)],
                space: AddressSpace::System,
                uncached: true,
                snooped: false,
            },
            false,
        )?;
        self.insert(mapping, Some(CpuLocation::Controller(controller)))
    }
    pub fn allocate(
        &mut self,
        size: u64,
        options: BufferOptions,
        pool: &mut SystemPool,
    ) -> Result<Buffer, Error> {
        if size == 0 {
            return Err(Error::Contract("zero GPU buffer allocation"));
        }
        if (options.host
            || (!self.asic.hw.bus.custom_bridge() && options.uncached && options.cpu_access))
            && pool
                .next
                .checked_add(size)
                .is_some_and(|end| end < pool.buffer.size)
        {
            let result = pool.buffer.view(pool.next, size)?;
            pool.next += size;
            return Ok(result);
        }
        self.allocate_vram(size, options)
    }
    pub(crate) fn allocate_vram(
        &mut self,
        size: u64,
        options: BufferOptions,
    ) -> Result<Buffer, Error> {
        if size == 0 {
            return Err(Error::Contract("zero GPU buffer allocation"));
        }
        let alignment = if size >= 8 << 20 { 2 << 20 } else { 4096 };
        let size = size
            .checked_next_multiple_of(alignment)
            .ok_or(Error::Contract("GPU buffer size overflow"))?;
        let mapping = self.asic.memory.allocate(
            &mut self.asic.hw,
            size,
            4096,
            options.uncached,
            options.cpu_access,
        )?;
        let cpu = if options.cpu_access {
            Some(CpuLocation::Vram(mapping.physical[0].0))
        } else {
            None
        };
        self.insert(mapping, cpu)
    }
    fn allocation(&self, buffer: Buffer) -> Result<&Allocation, Error> {
        let allocation = self
            .allocations
            .get(&buffer.id)
            .ok_or(Error::Contract("GPU buffer has been freed"))?;
        if buffer
            .offset
            .checked_add(buffer.size)
            .is_none_or(|end| end > allocation.mapping.size)
        {
            return Err(Error::Contract("GPU buffer outside allocation"));
        }
        Ok(allocation)
    }
    pub fn cpu(&self, buffer: Buffer) -> Result<CpuLocation, Error> {
        self.allocation(buffer)?
            .cpu
            .ok_or(Error::Contract("GPU buffer has no CPU mapping"))?
            .offset(buffer.offset)
    }
    pub fn write(&mut self, buffer: Buffer, bytes: &[u8]) -> Result<(), Error> {
        if bytes.len() as u64 > buffer.size {
            return Err(Error::Contract("CPU write exceeds GPU buffer"));
        }
        self.cpu(buffer)?.write(&mut self.asic.hw.bus, bytes)
    }
    pub fn read(&mut self, buffer: Buffer, size: usize) -> Result<Vec<u8>, Error> {
        if size as u64 > buffer.size {
            return Err(Error::Contract("CPU read exceeds GPU buffer"));
        }
        self.cpu(buffer)?.read(&mut self.asic.hw.bus, size)
    }
    pub fn write_scalar(&mut self, buffer: Buffer, size: u8, value: u64) -> Result<(), Error> {
        if u64::from(size) > buffer.size {
            return Err(Error::Contract("CPU scalar exceeds GPU buffer"));
        }
        self.cpu(buffer)?
            .write_scalar(&mut self.asic.hw.bus, size, value)
    }
    pub fn read_scalar(&mut self, buffer: Buffer, size: u8) -> Result<u64, Error> {
        if u64::from(size) > buffer.size {
            return Err(Error::Contract("CPU scalar exceeds GPU buffer"));
        }
        self.cpu(buffer)?.read_scalar(&mut self.asic.hw.bus, size)
    }
    pub fn cache(&mut self, buffer: Buffer) -> Result<(), Error> {
        if let CpuLocation::Vram(address) = self.cpu(buffer)? {
            self.asic.hw.bus.cache_vram(address, buffer.size)?;
        }
        Ok(())
    }
    pub fn free(&mut self, buffer: Buffer) -> Result<(), Error> {
        let allocation = self.allocation(buffer)?;
        if allocation.mapping.space != AddressSpace::Physical {
            return Ok(());
        }
        if buffer.offset != 0 || buffer.size != allocation.mapping.size {
            return Err(Error::Contract("cannot free GPU buffer view"));
        }
        let allocation = self
            .allocations
            .remove(&buffer.id)
            .ok_or(Error::Contract("GPU allocation disappeared"))?;
        self.asic.memory.free(&mut self.asic.hw, allocation.mapping)
    }
}
