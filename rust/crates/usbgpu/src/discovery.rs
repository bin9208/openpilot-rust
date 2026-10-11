use crate::{
    amd_metadata::{Catalog, DataType, Register},
    Error,
};
use serde::Serialize;
use std::collections::BTreeMap;
#[derive(Debug, Serialize)]
pub struct Discovery {
    pub versions: BTreeMap<u32, [u8; 3]>,
    pub bases: BTreeMap<u32, BTreeMap<u8, Vec<u64>>>,
    pub gc_version: [u16; 2],
    pub gc_info: BTreeMap<String, u64>,
}
fn slice(bytes: &[u8], offset: usize) -> Result<&[u8], Error> {
    bytes
        .get(offset..)
        .ok_or(Error::Contract("AMD discovery offset outside table"))
}
impl Discovery {
    pub fn parse(catalog: &Catalog, bytes: &[u8]) -> Result<Self, Error> {
        if catalog.read("struct_binary_header", &["binary_signature"], bytes)?
            != catalog.constant("am", "BINARY_SIGNATURE")?
        {
            return Err(Error::Contract("AMD binary discovery signature mismatch"));
        }
        let (_, tables) = catalog.field("struct_binary_header", &["table_list"])?;
        let table_size = catalog.layout("struct_table_info")?.size;
        let ip_offset =
            catalog.read("struct_table_info", &["offset"], slice(bytes, tables)?)? as usize;
        let gc_offset = catalog.read(
            "struct_table_info",
            &["offset"],
            slice(bytes, tables + table_size)?,
        )? as usize;
        let header = slice(bytes, ip_offset)?;
        if catalog.read("struct_ip_discovery_header", &["signature"], header)?
            != catalog.constant("am", "DISCOVERY_TABLE_SIGNATURE")?
        {
            return Err(Error::Contract("AMD IP discovery signature mismatch"));
        }
        let dies = catalog.read("struct_ip_discovery_header", &["num_dies"], header)? as usize;
        if dies > 16 {
            return Err(Error::Contract("AMD discovery die count exceeds header"));
        }
        let wide = catalog.read("struct_ip_discovery_header", &["base_addr_64_bit"], header)? != 0;
        let (_, die_info) = catalog.field("struct_ip_discovery_header", &["die_info"])?;
        let mut versions = BTreeMap::new();
        let mut bases = BTreeMap::<u32, BTreeMap<u8, Vec<u64>>>::new();
        for die in 0..dies {
            let record = slice(
                header,
                die_info + die * catalog.layout("struct_die_info")?.size,
            )?;
            let offset = catalog.read("struct_die_info", &["die_offset"], record)? as usize;
            let count = catalog.read("struct_die_header", &["num_ips"], slice(bytes, offset)?)?;
            let mut cursor = offset + catalog.layout("struct_die_header")?.size;
            for _ in 0..count {
                let record = slice(bytes, cursor)?;
                let id = catalog.read("struct_ip_v4", &["hw_id"], record)? as u32;
                let instance = catalog.read("struct_ip_v4", &["instance_number"], record)? as u8;
                let count = catalog.read("struct_ip_v4", &["num_base_address"], record)? as usize;
                let version = [
                    catalog.read("struct_ip_v4", &["major"], record)? as u8,
                    catalog.read("struct_ip_v4", &["minor"], record)? as u8,
                    catalog.read("struct_ip_v4", &["revision"], record)? as u8,
                ];
                let width = if wide { 8 } else { 4 };
                let mut segments = Vec::with_capacity(count);
                for index in 0..count {
                    let start = 8 + index * width;
                    let data = record
                        .get(start..start + width)
                        .ok_or(Error::Contract("truncated AMD discovery bases"))?;
                    let mut raw = [0; 8];
                    raw[..width].copy_from_slice(data);
                    segments.push(u64::from_le_bytes(raw));
                }
                for (&hardware, &mapped_id) in &catalog.hardware_ids {
                    if id == mapped_id {
                        versions.insert(hardware, version);
                        bases
                            .entry(hardware)
                            .or_default()
                            .insert(instance, segments.clone());
                    }
                }
                cursor = cursor
                    .checked_add(8 + count * width)
                    .ok_or(Error::Contract("AMD discovery cursor overflow"))?;
            }
        }
        let gc = slice(bytes, gc_offset)?;
        let gc_version = [
            catalog.read("struct_gpu_info_header", &["version_major"], gc)? as u16,
            catalog.read("struct_gpu_info_header", &["version_minor"], gc)? as u16,
        ];
        let layout = format!("struct_gc_info_v{}_{}", gc_version[0], gc_version[1]);
        let mut gc_info = BTreeMap::new();
        for (name, field) in &catalog.layout(&layout)?.fields {
            if matches!(field.datatype, DataType::Integer { .. }) {
                gc_info.insert(name.clone(), catalog.read(&layout, &[name], gc)?);
            }
        }
        Ok(Self {
            versions,
            bases,
            gc_version,
            gc_info,
        })
    }
    pub fn version(&self, hardware: u32) -> Result<[u8; 3], Error> {
        self.versions
            .get(&hardware)
            .copied()
            .ok_or(Error::Contract("AMD hardware IP missing"))
    }
    pub fn gc_version(&self) -> Result<[u8; 3], Error> {
        self.version(1)
    }
    pub fn reserved_vram(&self) -> Result<u64, Error> {
        let version = self.gc_version()?;
        Ok(if version[0] == 9 && [4, 5].contains(&version[1]) {
            384 << 20
        } else {
            64 << 20
        })
    }
    pub fn registers(&self, catalog: &Catalog) -> Result<BTreeMap<String, Register>, Error> {
        let mut modules = vec![
            ("mp", 15),
            ("hdp", 2),
            ("gc", 1),
            ("mmhub", 12),
            ("osssys", 23),
            (
                if self.gc_version()? < [12, 0, 0] {
                    "nbio"
                } else {
                    "nbif"
                },
                14,
            ),
        ];
        if [[4, 4, 2], [4, 4, 4]].contains(&self.version(3)?) {
            modules.push(("sdma", 3));
        }
        let mut registers = BTreeMap::new();
        for (prefix, hardware) in modules {
            let definitions = catalog.register_module(prefix, self.version(hardware)?)?;
            let bases = self
                .bases
                .get(&hardware)
                .ok_or(Error::Contract("AMD register bases missing"))?;
            for (name, definition) in definitions {
                registers.insert(name.clone(), Register::bind(definition, bases)?);
            }
        }
        let bases = self
            .bases
            .get(&16)
            .ok_or(Error::Contract("AMD MP1 register bases missing"))?;
        for (name, definition) in catalog.register_module("mp", [11, 0, 0])? {
            registers.insert(name.clone(), Register::bind(definition, bases)?);
        }
        Ok(registers)
    }
}
