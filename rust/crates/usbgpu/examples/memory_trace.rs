use openpilot_usbgpu::{
    allocator::Tlsf,
    memory::{Mapping, Memory, MemoryIo},
    page_table::{AddressSpace, PtePolicy},
    Error,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    io::{self, BufRead},
    sync::{Arc, Mutex},
};
#[derive(Default)]
struct Io {
    entries: BTreeMap<u64, u64>,
    trace: Vec<Value>,
}
impl MemoryIo for Io {
    fn read_entry(&mut self, address: u64) -> Result<u64, Error> {
        Ok(self.entries.get(&address).copied().unwrap_or(0))
    }
    fn write_entry(&mut self, address: u64, value: u64) -> Result<(), Error> {
        self.entries.insert(address, value);
        self.trace.push(json!(["write", address, value]));
        Ok(())
    }
    fn zero(&mut self, address: u64, size: u64) -> Result<(), Error> {
        self.entries
            .retain(|&a, _| a < address || a >= address + size);
        self.trace.push(json!(["zero", address, size]));
        Ok(())
    }
    fn mappings_changed(&mut self) -> Result<(), Error> {
        self.trace.push(json!(["flush"]));
        Ok(())
    }
}
fn mapping(m: &Mapping) -> Value {
    json!({"address":m.address,"size":m.size,"physical":m.physical,"uncached":m.uncached,"snooped":m.snooped})
}
fn main() {
    for line in io::stdin().lock().lines() {
        let v: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let mut io = Io::default();
        let va = Arc::new(Mutex::new(Tlsf::new(1 << 44, 0x200000000000).unwrap()));
        let mut memory = Memory::new(
            &mut io,
            512 << 20,
            32 << 20,
            v["reserve"].as_bool().unwrap(),
            PtePolicy {
                gfx_major: v["gfx"].as_u64().unwrap() as u8,
                uncached_type: 3,
                address_mask: (1 << 44) - 1,
                physical_base: 0,
            },
            va,
            false,
        )
        .unwrap();
        memory.gmmu = v["gmmu"].as_bool().unwrap();
        memory.is_booting = false;
        let mut allocations = HashMap::new();
        let mut results = Vec::new();
        for operation in v["operations"].as_array().unwrap() {
            let id = operation["id"].as_u64().unwrap();
            match operation["kind"].as_str().unwrap() {
                "alloc" => {
                    let m = memory
                        .allocate(
                            &mut io,
                            operation["size"].as_u64().unwrap(),
                            operation["align"].as_u64().unwrap(),
                            operation["uncached"].as_bool().unwrap(),
                            operation["contiguous"].as_bool().unwrap(),
                        )
                        .unwrap();
                    results.push(mapping(&m));
                    allocations.insert(id, m);
                }
                "free" => {
                    memory
                        .free(&mut io, allocations.remove(&id).unwrap())
                        .unwrap();
                    results.push(Value::Null);
                }
                "map_system" => {
                    let address = memory.alloc_virtual(8192, 4096).unwrap();
                    let m = memory
                        .map(
                            &mut io,
                            Mapping {
                                address,
                                size: 8192,
                                physical: vec![(0x200000, 4096), (0x201000, 4096)],
                                space: AddressSpace::System,
                                uncached: true,
                                snooped: true,
                            },
                            false,
                        )
                        .unwrap();
                    results.push(mapping(&m));
                    allocations.insert(id, m);
                }
                "unmap" => {
                    let m = allocations.remove(&id).unwrap();
                    memory.unmap(&mut io, m.address, m.size).unwrap();
                    results.push(Value::Null);
                }
                _ => panic!("unknown memory operation"),
            }
        }
        println!("{}", json!({"results":results,"writes":io.trace}));
    }
}
