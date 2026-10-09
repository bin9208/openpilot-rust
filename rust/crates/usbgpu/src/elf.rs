use crate::Error;
use serde::Serialize;
const MAX_IMAGE: usize = 512 << 20;
#[derive(Clone, Debug, Serialize)]
pub struct Section {
    pub name: String,
    pub kind: u32,
    pub address: u64,
    pub offset: u64,
    pub size: u64,
    pub alignment: u64,
    pub entry_size: u64,
}
#[derive(Clone, Debug, Serialize)]
pub struct Relocation {
    pub offset: u64,
    pub symbol: u64,
    pub kind: u32,
    pub addend: i64,
}
pub struct Image {
    pub bytes: Vec<u8>,
    pub sections: Vec<Section>,
    pub relocations: Vec<Relocation>,
}
fn slice(data: &[u8], offset: u64, size: u64) -> Result<&[u8], Error> {
    let start = usize::try_from(offset).map_err(|_| Error::Contract("ELF offset overflow"))?;
    let count = usize::try_from(size).map_err(|_| Error::Contract("ELF size overflow"))?;
    data.get(
        start
            ..start
                .checked_add(count)
                .ok_or(Error::Contract("ELF range overflow"))?,
    )
    .ok_or(Error::Contract("truncated ELF data"))
}
fn number(data: &[u8], offset: u64, size: u64) -> Result<u64, Error> {
    let data = slice(data, offset, size)?;
    let mut bytes = [0; 8];
    bytes[..data.len()].copy_from_slice(data);
    Ok(u64::from_le_bytes(bytes))
}
fn string(data: &[u8], offset: u64) -> Result<String, Error> {
    let tail = data
        .get(usize::try_from(offset).map_err(|_| Error::Contract("ELF string offset overflow"))?..)
        .ok_or(Error::Contract("ELF string offset outside table"))?;
    let end = tail
        .iter()
        .position(|&b| b == 0)
        .ok_or(Error::Contract("unterminated ELF string"))?;
    String::from_utf8(tail[..end].to_vec()).map_err(Error::from)
}
fn image_end(address: u64, size: u64) -> Result<usize, Error> {
    let end = address
        .checked_add(size)
        .ok_or(Error::Contract("ELF image overflow"))?;
    if end > MAX_IMAGE as u64 {
        return Err(Error::Contract("ELF image exceeds 512 MiB"));
    }
    Ok(end as usize)
}
impl Image {
    pub fn parse(blob: &[u8]) -> Result<Self, Error> {
        if blob.get(..4) != Some(b"\x7fELF") || blob.get(5) != Some(&1) {
            return Err(Error::Contract("expected little-endian ELF"));
        }
        let wide = match blob.get(4) {
            Some(1) => false,
            Some(2) => true,
            _ => return Err(Error::Contract("unknown ELF class")),
        };
        let shoff = number(blob, if wide { 40 } else { 32 }, if wide { 8 } else { 4 })?;
        let entry_size = number(blob, if wide { 58 } else { 46 }, 2)?;
        let count = number(blob, if wide { 60 } else { 48 }, 2)?;
        let strings = number(blob, if wide { 62 } else { 50 }, 2)? as usize;
        if entry_size != if wide { 64 } else { 40 } {
            return Err(Error::Contract("unexpected ELF section header size"));
        }
        let headers = slice(blob, shoff, entry_size * count)?;
        let mut names = Vec::new();
        let mut sections = Vec::new();
        for header in headers.chunks_exact(entry_size as usize) {
            names.push(number(header, 0, 4)?);
            sections.push(Section {
                name: String::new(),
                kind: number(header, 4, 4)? as u32,
                address: number(header, if wide { 16 } else { 12 }, if wide { 8 } else { 4 })?,
                offset: number(header, if wide { 24 } else { 16 }, if wide { 8 } else { 4 })?,
                size: number(header, if wide { 32 } else { 20 }, if wide { 8 } else { 4 })?,
                alignment: number(header, if wide { 48 } else { 32 }, if wide { 8 } else { 4 })?,
                entry_size: number(header, if wide { 56 } else { 36 }, if wide { 8 } else { 4 })?,
            });
        }
        let str_section = sections
            .get(strings)
            .ok_or(Error::Contract("ELF section-name table missing"))?;
        let strtab = slice(blob, str_section.offset, str_section.size)?;
        for (section, name) in sections.iter_mut().zip(names) {
            section.name = string(strtab, name)?;
        }
        let mut size = 0;
        for section in &sections {
            if section.kind == 1 && section.address != 0 {
                size = size.max(image_end(section.address, section.size)?);
            }
        }
        let mut bytes = vec![0; size];
        for section in &mut sections {
            if section.kind != 1 {
                continue;
            }
            let content = slice(blob, section.offset, section.size)?;
            if section.address == 0 {
                let alignment = section.alignment.max(1);
                let padding = (alignment - bytes.len() as u64 % alignment) % alignment;
                let start = image_end(bytes.len() as u64, padding)?;
                let end = image_end(start as u64, section.size)?;
                bytes.resize(end, 0);
                section.address = start as u64;
            }
            let start = section.address as usize;
            bytes[start..start + content.len()].copy_from_slice(content);
        }
        let symbols = sections.iter().find(|s| s.kind == 2);
        let mut relocations = Vec::new();
        for kind in [9, 4] {
            for section in sections.iter().filter(|s| s.kind == kind) {
                let target_name = section
                    .name
                    .get(if kind == 9 { 4 } else { 5 }..)
                    .ok_or(Error::Contract("invalid ELF relocation section name"))?;
                if target_name == ".eh_frame" {
                    continue;
                }
                let target = sections
                    .iter()
                    .find(|s| s.name == target_name)
                    .ok_or(Error::Contract("ELF relocation target missing"))?;
                let symbols = symbols.ok_or(Error::Contract("ELF symbol table missing"))?;
                let expected = if wide {
                    if kind == 9 {
                        16
                    } else {
                        24
                    }
                } else if kind == 9 {
                    8
                } else {
                    12
                };
                if section.entry_size != expected
                    || section.size % expected != 0
                    || symbols.entry_size != if wide { 24 } else { 16 }
                {
                    return Err(Error::Contract("invalid ELF relocation/symbol entry size"));
                }
                for entry in
                    slice(blob, section.offset, section.size)?.chunks_exact(expected as usize)
                {
                    let width = if wide { 8 } else { 4 };
                    let offset = number(entry, 0, width)?;
                    let info = number(entry, width, width)?;
                    let index = if wide { info >> 32 } else { info >> 8 };
                    let rtype = if wide {
                        info as u32
                    } else {
                        (info & 255) as u32
                    };
                    let symbol = slice(
                        slice(blob, symbols.offset, symbols.size)?,
                        index
                            .checked_mul(symbols.entry_size)
                            .ok_or(Error::Contract("ELF symbol index overflow"))?,
                        symbols.entry_size,
                    )?;
                    let section_index = number(symbol, if wide { 6 } else { 14 }, 2)? as usize;
                    if section_index == 0 {
                        return Err(Error::Contract("external ELF symbol is not a GPU address"));
                    }
                    let symbol_value = number(symbol, if wide { 8 } else { 4 }, width)?;
                    let symbol_address = sections
                        .get(section_index)
                        .ok_or(Error::Contract("ELF symbol section missing"))?
                        .address
                        .checked_add(symbol_value)
                        .ok_or(Error::Contract("ELF symbol address overflow"))?;
                    let addend = if kind == 9 {
                        0
                    } else if wide {
                        number(entry, 16, 8)? as i64
                    } else {
                        number(entry, 8, 4)? as i32 as i64
                    };
                    relocations.push(Relocation {
                        offset: target
                            .address
                            .checked_add(offset)
                            .ok_or(Error::Contract("ELF relocation overflow"))?,
                        symbol: symbol_address,
                        kind: rtype,
                        addend,
                    });
                }
            }
        }
        Ok(Self {
            bytes,
            sections,
            relocations,
        })
    }
    pub fn relocate_amd(&mut self) -> Result<(), Error> {
        for relocation in &self.relocations {
            if relocation.kind != 5 {
                return Err(Error::Protocol(format!(
                    "unknown AMD reloc {}",
                    relocation.kind
                )));
            }
            let value = i128::from(relocation.symbol) - i128::from(relocation.offset)
                + i128::from(relocation.addend);
            let value = i64::try_from(value).map_err(|_| Error::Contract("AMD REL64 overflow"))?;
            let start = usize::try_from(relocation.offset)
                .map_err(|_| Error::Contract("AMD relocation offset overflow"))?;
            let end = start
                .checked_add(8)
                .ok_or(Error::Contract("AMD relocation range overflow"))?;
            self.bytes
                .get_mut(start..end)
                .ok_or(Error::Contract("AMD relocation outside image"))?
                .copy_from_slice(&value.to_le_bytes());
        }
        Ok(())
    }
}
