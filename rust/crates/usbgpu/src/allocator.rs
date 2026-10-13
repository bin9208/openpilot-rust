use crate::Error;
use std::collections::{BTreeMap, HashMap, VecDeque};
#[derive(Clone, Copy)]
struct Block {
    size: u64,
    next: u64,
    previous: Option<u64>,
    free: bool,
}
pub struct Tlsf {
    pub size: u64,
    pub base: u64,
    blocks: HashMap<u64, Block>,
    storage: Vec<BTreeMap<u32, VecDeque<u64>>>,
}
fn bits(value: u64) -> u32 {
    64 - value.leading_zeros()
}
fn second(value: u64) -> u32 {
    ((value - (1 << (bits(value) - 1))) >> (bits(value).saturating_sub(5))) as u32
}
fn round(value: u64, alignment: u64) -> Result<u64, Error> {
    if alignment == 0 {
        return Err(Error::Contract("zero allocator alignment"));
    }
    value
        .checked_next_multiple_of(alignment)
        .ok_or(Error::Contract("allocator size overflow"))
}
impl Tlsf {
    pub fn new(size: u64, base: u64) -> Result<Self, Error> {
        base.checked_add(size)
            .ok_or(Error::Contract("allocator range overflow"))?;
        let mut result = Self {
            size,
            base,
            blocks: HashMap::from([(
                0,
                Block {
                    size,
                    next: size,
                    previous: None,
                    free: true,
                },
            )]),
            storage: vec![BTreeMap::new(); bits(size) as usize + 1],
        };
        if size > 0 {
            result.insert(0, size, None)?;
        }
        Ok(result)
    }
    fn insert(&mut self, start: u64, size: u64, previous: Option<u64>) -> Result<(), Error> {
        if size == 0 {
            return Err(Error::Contract("zero free block"));
        }
        let previous = previous.or_else(|| self.blocks.get(&start).and_then(|b| b.previous));
        self.storage[bits(size) as usize]
            .entry(second(size))
            .or_default()
            .push_back(start);
        self.blocks.insert(
            start,
            Block {
                size,
                next: start + size,
                previous,
                free: true,
            },
        );
        Ok(())
    }
    fn remove(&mut self, start: u64, size: u64) -> Result<(), Error> {
        let bucket = self.storage[bits(size) as usize]
            .get_mut(&second(size))
            .ok_or(Error::Contract("missing allocator bucket"))?;
        let position = bucket
            .iter()
            .position(|&entry| entry == start)
            .ok_or(Error::Contract("missing free block"))?;
        bucket.remove(position);
        self.blocks
            .get_mut(&start)
            .ok_or(Error::Contract("missing allocation block"))?
            .free = false;
        Ok(())
    }
    fn split(&mut self, start: u64, new_size: u64) -> Result<(), Error> {
        let block = *self
            .blocks
            .get(&start)
            .ok_or(Error::Contract("missing split block"))?;
        if !block.free || new_size == 0 || new_size >= block.size {
            return Err(Error::Contract("invalid allocator split"));
        }
        self.remove(start, block.size)?;
        self.insert(start, new_size, None)?;
        self.insert(start + new_size, block.size - new_size, Some(start))?;
        if let Some(next) = self.blocks.get_mut(&block.next) {
            next.previous = Some(start + new_size);
        }
        Ok(())
    }
    pub fn alloc(&mut self, requested: u64, alignment: u64) -> Result<u64, Error> {
        if alignment == 0 {
            return Err(Error::Contract("zero allocator alignment"));
        }
        let requested = requested.max(16);
        let padded = requested
            .checked_add(alignment - 1)
            .ok_or(Error::Contract("allocation padding overflow"))?;
        let size = round(padded, 1 << bits(padded).saturating_sub(5))?;
        for level in bits(size) as usize..self.storage.len() {
            let lower = if level == bits(size) as usize {
                second(size)
            } else {
                0
            };
            let start = self.storage[level]
                .range(lower..32)
                .find_map(|(_, entries)| entries.front().copied());
            if let Some(mut start) = start {
                let available = self.blocks[&start].size;
                if available < size {
                    return Err(Error::Contract("allocator bucket is too small"));
                }
                let aligned = round(start, alignment)?;
                if aligned != start {
                    self.split(start, aligned - start)?;
                    start = aligned;
                }
                if self.blocks[&start].size > requested {
                    self.split(start, requested)?;
                }
                self.remove(start, requested)?;
                return Ok(start + self.base);
            }
        }
        Err(Error::Allocation(requested))
    }
    pub fn free(&mut self, address: u64) -> Result<(), Error> {
        let mut start = address
            .checked_sub(self.base)
            .ok_or(Error::Contract("free address below allocator base"))?;
        let block = *self
            .blocks
            .get(&start)
            .ok_or(Error::Contract("free address is not allocated"))?;
        if block.free {
            return Err(Error::Contract("double free"));
        }
        self.insert(start, block.size, None)?;
        while let Some(previous) = self.blocks[&start].previous {
            if !self.blocks[&previous].free {
                break;
            }
            start = previous;
        }
        loop {
            let block = self.blocks[&start];
            let Some(next) = self.blocks.get(&block.next).copied() else {
                break;
            };
            if !next.free {
                break;
            }
            self.remove(start, block.size)?;
            self.remove(block.next, next.size)?;
            self.insert(start, block.size + next.size, None)?;
            self.blocks.remove(&block.next);
        }
        let next_address = self.blocks[&start].next;
        if let Some(next) = self.blocks.get_mut(&next_address) {
            next.previous = Some(start);
        }
        Ok(())
    }
}
