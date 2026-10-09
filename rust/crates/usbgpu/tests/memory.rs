use openpilot_usbgpu::{
    allocator::Tlsf,
    memory::{Memory, MemoryIo},
    page_table::{PteOptions, PtePolicy},
    Error,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
#[derive(Default)]
struct Io {
    words: BTreeMap<u64, u64>,
    flushes: usize,
}
impl MemoryIo for Io {
    fn read_entry(&mut self, address: u64) -> Result<u64, Error> {
        Ok(self.words.get(&address).copied().unwrap_or(0))
    }
    fn write_entry(&mut self, address: u64, value: u64) -> Result<(), Error> {
        self.words.insert(address, value);
        Ok(())
    }
    fn zero(&mut self, address: u64, size: u64) -> Result<(), Error> {
        self.words
            .retain(|&key, _| key < address || key >= address + size);
        Ok(())
    }
    fn mappings_changed(&mut self) -> Result<(), Error> {
        self.flushes += 1;
        Ok(())
    }
}
fn policy() -> PtePolicy {
    PtePolicy {
        gfx_major: 12,
        uncached_type: 3,
        address_mask: (1 << 44) - 1,
        physical_base: 0,
    }
}
#[test]
fn allocation_and_mapping_are_reusable_after_free() {
    let mut io = Io::default();
    let va = Arc::new(Mutex::new(Tlsf::new(1 << 44, 0x200000000000).unwrap()));
    let mut memory = Memory::new(&mut io, 128 << 20, 32 << 20, true, policy(), va, false).unwrap();
    memory.is_booting = false;
    let first = memory
        .allocate(&mut io, 2 << 20, 4096, false, false)
        .unwrap();
    let first_address = first.address;
    let second = memory.allocate(&mut io, 4096, 4096, true, true).unwrap();
    assert_ne!(first.address, second.address);
    assert!(memory.map(&mut io, second.clone(), false).is_err());
    assert_eq!(io.flushes, 2);
    memory.free(&mut io, first).unwrap();
    memory.free(&mut io, second).unwrap();
    let reused = memory
        .allocate(&mut io, 2 << 20, 4096, false, false)
        .unwrap();
    assert_eq!(reused.address, first_address);
    assert_eq!(io.flushes, 3);
    memory.free(&mut io, reused).unwrap();
    assert!(memory.unmap(&mut io, first_address, 4096).is_err());
}
#[test]
fn allocator_rejects_double_free_and_restores_capacity() {
    let mut allocator = Tlsf::new(65536, 0x10000).unwrap();
    let mut blocks = Vec::new();
    for _ in 0..8 {
        blocks.push(allocator.alloc(4096, 4096).unwrap());
    }
    assert!(blocks.windows(2).all(|pair| pair[1] >= pair[0] + 4096));
    for &address in &blocks {
        allocator.free(address).unwrap();
    }
    assert!(allocator.free(blocks[0]).is_err());
    assert_eq!(allocator.alloc(65536, 1).unwrap(), 0x10000);
}
#[test]
fn pte_rejects_physical_addresses_outside_gpu_aperture() {
    assert!(policy().entry(3, 1 << 44, PteOptions::default()).is_err());
    assert_eq!(
        policy().entry(3, 4096, PteOptions::default()).unwrap(),
        (1 << 63) | 4096 | 0x71
    );
}
