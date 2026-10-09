use crate::{
    allocator::Tlsf,
    page_table::{fragment, AddressSpace, PteOptions, PtePolicy},
    Error,
};
use std::sync::{Arc, Mutex};
pub trait MemoryIo {
    fn read_entry(&mut self, address: u64) -> Result<u64, Error>;
    fn write_entry(&mut self, address: u64, value: u64) -> Result<(), Error>;
    fn zero(&mut self, address: u64, size: u64) -> Result<(), Error>;
    fn mappings_changed(&mut self) -> Result<(), Error>;
}
#[derive(Clone, Debug)]
pub struct Mapping {
    pub address: u64,
    pub size: u64,
    pub physical: Vec<(u64, u64)>,
    pub space: AddressSpace,
    pub uncached: bool,
    pub snooped: bool,
}
#[derive(Clone, Copy)]
struct Table {
    address: u64,
    level: u8,
}
#[derive(Clone, Copy)]
struct Frame {
    table: Table,
    index: u64,
}
#[derive(Clone, Copy)]
enum Mode {
    Create,
    Free,
    Inspect,
}
struct Cursor {
    address: u64,
    frames: Vec<Frame>,
    mode: Mode,
    boot: bool,
}
#[derive(Clone, Copy)]
struct Segment {
    frame: Frame,
    count: u64,
    cover: u64,
}
fn cover(level: u8) -> u64 {
    1 << (39 - u32::from(level) * 9)
}
fn count(level: u8) -> u64 {
    if level == 0 {
        1024
    } else {
        512
    }
}
fn entry_address(frame: Frame) -> u64 {
    frame.table.address + frame.index * 8
}
impl Cursor {
    fn new(memory: &Memory, address: u64, mode: Mode, boot: bool) -> Result<Self, Error> {
        let address = address
            .checked_sub(memory.virtual_base)
            .ok_or(Error::Contract("mapping below virtual aperture"))?;
        if address >= (1 << 49) {
            return Err(Error::Contract("mapping exceeds virtual aperture"));
        }
        Ok(Self {
            address,
            frames: vec![Frame {
                table: memory.root,
                index: (address / cover(memory.root.level)) % count(memory.root.level),
            }],
            mode,
            boot,
        })
    }
    fn down(&mut self, memory: &mut Memory, io: &mut impl MemoryIo) -> Result<(), Error> {
        let frame = *self
            .frames
            .last()
            .ok_or(Error::Contract("empty page table traversal"))?;
        if frame.table.level >= 3 {
            return Err(Error::Contract("page table traversal below leaf"));
        }
        let mut entry = io.read_entry(entry_address(frame))?;
        if entry & 1 == 0 {
            if !matches!(self.mode, Mode::Create) {
                return Err(Error::Contract("missing page table"));
            }
            let address = memory.palloc(io, 4096, 4096, true, self.boot, true)?;
            entry = memory.policy.entry(
                frame.table.level,
                address,
                PteOptions {
                    table: true,
                    ..PteOptions::default()
                },
            )?;
            io.write_entry(entry_address(frame), entry)?;
        }
        if memory.policy.is_page(frame.table.level, entry) {
            return Err(Error::Contract("page table traversal encountered a page"));
        }
        let table = Table {
            address: memory.policy.address(entry)?,
            level: frame.table.level + 1,
        };
        self.frames.push(Frame {
            table,
            index: (self.address / cover(table.level)) % count(table.level),
        });
        Ok(())
    }
    fn locate(
        &mut self,
        memory: &mut Memory,
        io: &mut impl MemoryIo,
        size: u64,
    ) -> Result<Segment, Error> {
        loop {
            let frame = *self
                .frames
                .last()
                .ok_or(Error::Contract("empty page table traversal"))?;
            let covers = cover(frame.table.level);
            let down = match self.mode {
                Mode::Create => covers > size || self.address & (covers - 1) != 0,
                Mode::Free => {
                    frame.table.level != 3
                        && !memory
                            .policy
                            .is_page(frame.table.level, io.read_entry(entry_address(frame))?)
                }
                Mode::Inspect if frame.table.level == 3 => false,
                Mode::Inspect => {
                    let entry = io.read_entry(entry_address(frame))?;
                    !memory.policy.is_page(frame.table.level, entry) && entry & 1 != 0
                }
            };
            if down {
                self.down(memory, io)?;
                continue;
            }
            let entries = (size / covers)
                .min(count(frame.table.level) - frame.index)
                .max(u64::from(matches!(self.mode, Mode::Inspect)));
            if entries == 0 {
                return Err(Error::Contract("empty page table segment"));
            }
            return Ok(Segment {
                frame,
                count: entries,
                cover: covers,
            });
        }
    }
    fn advance(
        &mut self,
        memory: &mut Memory,
        io: &mut impl MemoryIo,
        segment: Segment,
    ) -> Result<(), Error> {
        self.address += segment.count * segment.cover;
        self.frames
            .last_mut()
            .ok_or(Error::Contract("empty page table traversal"))?
            .index += segment.count;
        loop {
            let frame = *self
                .frames
                .last()
                .ok_or(Error::Contract("page table traversal overflow"))?;
            let mut freed = false;
            if matches!(self.mode, Mode::Free) && frame.table.address != memory.root.address {
                let mut empty = true;
                for index in 0..count(frame.table.level) {
                    if io.read_entry(frame.table.address + index * 8)? & 1 != 0 {
                        empty = false;
                        break;
                    }
                }
                if empty {
                    memory.pfree(frame.table.address, true)?;
                    let parent = self.frames[self.frames.len() - 2];
                    let invalid = memory.policy.entry(
                        parent.table.level,
                        0,
                        PteOptions {
                            valid: false,
                            ..PteOptions::default()
                        },
                    )?;
                    io.write_entry(entry_address(parent), invalid)?;
                    freed = true;
                }
            }
            let complete = frame.index == count(frame.table.level);
            if !freed && !complete {
                break;
            }
            if self.frames.len() == 1 {
                return Err(Error::Contract("page table traversal exhausted root"));
            }
            self.frames.pop();
            if complete {
                self.frames.last_mut().unwrap().index += 1;
            }
        }
        Ok(())
    }
}
pub struct Memory {
    pub vram_size: u64,
    pub virtual_base: u64,
    pub is_booting: bool,
    pub policy: PtePolicy,
    root: Table,
    boot_allocator: Tlsf,
    page_allocator: Tlsf,
    physical_allocator: Tlsf,
    virtual_allocator: Arc<Mutex<Tlsf>>,
    reserve_tables: bool,
    identity_cached: Option<u64>,
    identity_uncached: Option<u64>,
    pub gmmu: bool,
}
impl Memory {
    pub fn new(
        io: &mut impl MemoryIo,
        vram_size: u64,
        boot_size: u64,
        reserve_tables: bool,
        policy: PtePolicy,
        virtual_allocator: Arc<Mutex<Tlsf>>,
        smi: bool,
    ) -> Result<Self, Error> {
        let table_size = if reserve_tables {
            (vram_size / 512)
                .checked_next_multiple_of(1 << 20)
                .ok_or(Error::Contract("page table reserve overflow"))?
        } else {
            0
        };
        let offset = boot_size
            .checked_add(table_size)
            .ok_or(Error::Contract("VRAM reserve overflow"))?;
        let free = vram_size
            .checked_sub(offset)
            .ok_or(Error::Contract("VRAM smaller than boot reserve"))?;
        let virtual_base = virtual_allocator
            .lock()
            .map_err(|_| Error::Contract("virtual allocator poisoned"))?
            .base;
        let mut result = Self {
            vram_size,
            virtual_base,
            is_booting: true,
            policy,
            root: Table {
                address: 0,
                level: 0,
            },
            boot_allocator: Tlsf::new(boot_size, 0)?,
            page_allocator: Tlsf::new(table_size, boot_size)?,
            physical_allocator: Tlsf::new(free, offset)?,
            virtual_allocator,
            reserve_tables,
            identity_cached: None,
            identity_uncached: None,
            gmmu: true,
        };
        result.root.address = result.palloc(io, 4096, 4096, !smi, true, false)?;
        Ok(result)
    }
    pub fn root_address(&self) -> u64 {
        self.root.address
    }
    pub fn palloc(
        &mut self,
        io: &mut impl MemoryIo,
        size: u64,
        alignment: u64,
        zero: bool,
        boot: bool,
        page_table: bool,
    ) -> Result<u64, Error> {
        if self.is_booting != boot {
            return Err(Error::Contract("boot allocation phase mismatch"));
        }
        let allocator = if boot {
            &mut self.boot_allocator
        } else if self.reserve_tables && page_table {
            &mut self.page_allocator
        } else {
            &mut self.physical_allocator
        };
        let aligned = size
            .checked_next_multiple_of(4096)
            .ok_or(Error::Contract("physical allocation size overflow"))?;
        let address = allocator.alloc(aligned, alignment)?;
        if zero {
            io.zero(address, size)?;
        }
        Ok(address)
    }
    pub fn pfree(&mut self, address: u64, page_table: bool) -> Result<(), Error> {
        if self.reserve_tables && page_table {
            self.page_allocator.free(address)
        } else {
            self.physical_allocator.free(address)
        }
    }
    pub fn alloc_virtual(&self, size: u64, alignment: u64) -> Result<u64, Error> {
        if size == 0 {
            return Err(Error::Contract("zero virtual allocation"));
        }
        let size_alignment = 1 << (63 - size.leading_zeros());
        self.virtual_allocator
            .lock()
            .map_err(|_| Error::Contract("virtual allocator poisoned"))?
            .alloc(size, alignment.max(size_alignment))
    }
    pub fn map(
        &mut self,
        io: &mut impl MemoryIo,
        mapping: Mapping,
        boot: bool,
    ) -> Result<Mapping, Error> {
        let total = mapping
            .physical
            .iter()
            .try_fold(0u64, |sum, (_, size)| sum.checked_add(*size))
            .ok_or(Error::Contract("physical mapping size overflow"))?;
        if total != mapping.size {
            return Err(Error::Contract("physical mapping size mismatch"));
        }
        let mut cursor = Cursor::new(self, mapping.address, Mode::Inspect, boot)?;
        let mut remaining = mapping.size;
        while remaining > 0 {
            let segment = cursor.locate(self, io, remaining)?;
            for offset in 0..segment.count {
                if io.read_entry(entry_address(segment.frame) + offset * 8)? & 1 != 0 {
                    return Err(Error::Contract("PTE already mapped"));
                }
            }
            remaining = remaining.saturating_sub(segment.count * segment.cover);
            cursor.advance(self, io, segment)?;
        }
        let mut cursor = Cursor::new(self, mapping.address, Mode::Create, boot)?;
        for &(physical, size) in &mapping.physical {
            let mut remaining = size;
            let mut offset = 0;
            while remaining > 0 {
                let segment = cursor.locate(self, io, remaining)?;
                let span = segment.count * segment.cover;
                let flags = PteOptions {
                    uncached: mapping.uncached,
                    space: mapping.space,
                    snooped: mapping.snooped,
                    fragment: fragment(cursor.address + offset, span, true)?,
                    ..PteOptions::default()
                };
                for index in 0..segment.count {
                    let entry = self.policy.entry(
                        segment.frame.table.level,
                        physical + offset + index * segment.cover,
                        flags,
                    )?;
                    io.write_entry(entry_address(segment.frame) + index * 8, entry)?;
                }
                remaining -= span;
                offset += span;
                cursor.advance(self, io, segment)?;
            }
        }
        io.mappings_changed()?;
        Ok(mapping)
    }
    pub fn unmap(&mut self, io: &mut impl MemoryIo, address: u64, size: u64) -> Result<(), Error> {
        let mut cursor = Cursor::new(self, address, Mode::Free, false)?;
        let mut remaining = size;
        while remaining > 0 {
            let segment = cursor.locate(self, io, remaining)?;
            for index in 0..segment.count {
                let address = entry_address(segment.frame) + index * 8;
                if io.read_entry(address)? & 1 == 0 {
                    return Err(Error::Contract("PTE not mapped"));
                }
                io.write_entry(
                    address,
                    self.policy.entry(
                        segment.frame.table.level,
                        0,
                        PteOptions {
                            valid: false,
                            ..PteOptions::default()
                        },
                    )?,
                )?;
            }
            remaining -= segment.count * segment.cover;
            cursor.advance(self, io, segment)?;
        }
        Ok(())
    }
    pub fn allocate(
        &mut self,
        io: &mut impl MemoryIo,
        size: u64,
        alignment: u64,
        uncached: bool,
        contiguous: bool,
    ) -> Result<Mapping, Error> {
        let size = size
            .checked_next_multiple_of(4096)
            .ok_or(Error::Contract("GPU allocation size overflow"))?;
        if !self.gmmu {
            let physical = self.palloc(io, size, alignment, false, false, false)?;
            let cached = if uncached {
                self.identity_uncached
            } else {
                self.identity_cached
            };
            let base = if let Some(base) = cached {
                base
            } else {
                let base = self.alloc_virtual(self.vram_size, self.vram_size)?;
                self.map(
                    io,
                    Mapping {
                        address: base,
                        size: self.vram_size,
                        physical: vec![(0, self.vram_size)],
                        space: AddressSpace::Physical,
                        uncached,
                        snooped: false,
                    },
                    false,
                )?;
                if uncached {
                    self.identity_uncached = Some(base);
                } else {
                    self.identity_cached = Some(base);
                }
                base
            };
            return Ok(Mapping {
                address: base + physical,
                size,
                physical: vec![(physical, size)],
                space: AddressSpace::Physical,
                uncached,
                snooped: false,
            });
        }
        let address = self.alloc_virtual(size, alignment)?;
        let mut physical = Vec::new();
        if contiguous {
            physical.push((self.palloc(io, size, 4096, true, false, false)?, size));
        } else {
            let mut power = 27;
            let mut remaining = size;
            while remaining > 0 {
                while (1u64 << (power + 12)) > remaining {
                    power -= 1;
                }
                let segment_size = 1u64 << (power + 12);
                let segment_alignment = if power >= 9 { 2 << 20 } else { 4096 };
                match self.palloc(io, segment_size, segment_alignment, false, false, false) {
                    Ok(address) => {
                        physical.push((address, segment_size));
                        remaining -= segment_size;
                    }
                    Err(Error::Allocation(_)) if power > 0 => {
                        power -= 1;
                    }
                    Err(error) => {
                        for (address, _) in physical {
                            self.pfree(address, false)?;
                        }
                        return Err(error);
                    }
                }
            }
        }
        self.map(
            io,
            Mapping {
                address,
                size,
                physical,
                space: AddressSpace::Physical,
                uncached,
                snooped: false,
            },
            false,
        )
    }
    pub fn free(&mut self, io: &mut impl MemoryIo, mapping: Mapping) -> Result<(), Error> {
        if !self.gmmu {
            return self.pfree(mapping.physical[0].0, false);
        }
        self.unmap(io, mapping.address, mapping.size)?;
        self.virtual_allocator
            .lock()
            .map_err(|_| Error::Contract("virtual allocator poisoned"))?
            .free(mapping.address)?;
        for (address, _) in mapping.physical {
            self.pfree(address, false)?;
        }
        Ok(())
    }
}
