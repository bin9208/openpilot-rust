use crate::Error;
use std::collections::{BTreeMap, HashSet};
pub trait Config {
    fn read_config(&mut self, bus: u8, offset: u16, size: u8) -> Result<u32, Error>;
    fn write_config(&mut self, bus: u8, offset: u16, size: u8, value: u32) -> Result<(), Error>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bar {
    pub address: u64,
    pub size: u64,
}
pub fn setup_bars(
    config: &mut impl Config,
    gpu_bus: u8,
    mem_base: u64,
    prefetch_base: u64,
) -> Result<BTreeMap<u8, Bar>, Error> {
    for bus in 0..gpu_bus {
        config.write_config(
            bus,
            0x18,
            4,
            (u32::from(bus) + 1) << 8 | u32::from(gpu_bus) << 16,
        )?;
        for (offset, size, value) in [
            (0x20, 2, ((mem_base >> 16) & 0xffff) as u32),
            (0x22, 2, 0xffff),
            (0x24, 2, ((prefetch_base >> 16) & 0xffff) as u32),
            (0x26, 2, 0xffff),
            (0x28, 4, (prefetch_base >> 32) as u32),
            (0x2c, 4, u32::MAX),
            (4, 1, 7),
        ] {
            config.write_config(bus, offset, size, value)?;
        }
    }
    let mut pointer = 0x100;
    let mut visited = HashSet::new();
    while pointer != 0 {
        if !visited.insert(pointer) {
            return Err(Error::Contract("PCI extended capability cycle"));
        }
        let header = config.read_config(gpu_bus, pointer, 4)?;
        if header & 0xffff == 0x15 {
            let capability = config.read_config(gpu_bus, pointer + 4, 4)?;
            if capability >> 4 == 0 {
                return Err(Error::Contract("PCI resizable BAR has no supported size"));
            }
            let control = config.read_config(gpu_bus, pointer + 8, 4)?;
            let size_bit = 31 - (capability >> 4).leading_zeros();
            config.write_config(
                gpu_bus,
                pointer + 8,
                4,
                (control & !0x1f00) | (size_bit << 8),
            )?;
        }
        pointer = ((header >> 20) & 0xffc) as u16;
    }
    let mut bases = [mem_base, prefetch_base];
    let mut offset = 0;
    let mut bars = BTreeMap::new();
    while offset < 24 {
        let descriptor = config.read_config(gpu_bus, 0x10 + offset, 4)?;
        let prefetch = usize::from(descriptor & 8 != 0);
        let is_64 = descriptor & 4 != 0;
        if descriptor & 1 == 0 {
            config.write_config(gpu_bus, 0x10 + offset, 4, u32::MAX)?;
            let low = config.read_config(gpu_bus, 0x10 + offset, 4)? & 0xfffffff0;
            if is_64 {
                config.write_config(gpu_bus, 0x14 + offset, 4, u32::MAX)?;
            }
            let high = if is_64 {
                config.read_config(gpu_bus, 0x14 + offset, 4)?
            } else {
                0
            };
            let mask = (u64::from(high) << 32) | u64::from(low);
            let size = (!mask).wrapping_add(1) & if is_64 { u64::MAX } else { u64::from(u32::MAX) };
            config.write_config(gpu_bus, 0x10 + offset, 4, bases[prefetch] as u32)?;
            if is_64 {
                config.write_config(gpu_bus, 0x14 + offset, 4, (bases[prefetch] >> 32) as u32)?;
            }
            bars.insert(
                (offset / 4) as u8,
                Bar {
                    address: bases[prefetch],
                    size,
                },
            );
            let rounded = size
                .checked_next_multiple_of(2 << 20)
                .ok_or(Error::Contract("PCI BAR size overflow"))?;
            bases[prefetch] = bases[prefetch]
                .checked_add(rounded)
                .ok_or(Error::Contract("PCI BAR address overflow"))?;
        }
        offset += if is_64 { 8 } else { 4 };
    }
    config.write_config(gpu_bus, 4, 1, 7)?;
    Ok(bars)
}
