//! Mechanical ELF transport of authenticated, fully relocated x86-64 images.
//! This module never maps or calls an image and supplies no ISA/ABI admission.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_images_and_constant_tails_survive_container() {
        let images = [
            (7, &[0xc3, 0x91, 0x62][..]),
            (9, &[0xc3, 0x80, 0xff, 0][..]),
        ];
        let object = package(&images).unwrap();
        let functions = functions(&object, 1).unwrap();
        assert_eq!(functions.len(), 2);
        assert_eq!(functions["op_impl_7"].bytes, images[0].1);
        assert_eq!(functions["op_impl_9"].bytes, images[1].1);
        assert_eq!(functions["op_impl_9"].address % 64, 0);
    }

    #[test]
    fn packed_relocation_sections_reject_even_when_targeting_a_cached_image() {
        for kind in [
            19,
            0x4000_0014,
            0x6000_0001,
            0x6000_0002,
            0x6fff_ff00,
            0x7000_0004,
        ] {
            for address in [0_u64, 4096] {
                let mut object = package(&[(0, &[0xc3, 0x80])]).unwrap();
                let table = u64_at(&object, 40).unwrap() as usize;
                let offset = object.len();
                // A RELR direct entry at address zero names op_impl_0's bytes;
                // the second case proves unsupported formats reject regardless
                // of their target. The old scan silently ignored every section.
                object.extend_from_slice(&address.to_le_bytes());
                let section = &mut object[table + 5 * 64..table + 6 * 64];
                set32(section, 4, kind);
                set64(section, 8, 2);
                set64(section, 24, offset as u64);
                set64(section, 32, 8);
                set64(section, 48, 8);
                set64(section, 56, 8);
                assert!(reconstruct(&object).err().unwrap().contains("packed"));
                object[16..18].copy_from_slice(&3_u16.to_le_bytes());
                assert!(functions(&object, 3).err().unwrap().contains("packed"));
            }
        }
    }

    #[test]
    fn duplicate_symbols_empty_and_excessive_images_reject() {
        assert!(package(&[]).is_err());
        assert!(package(&[(0, &[])]).is_err());
        assert!(package(&[(0, &[0xc3]), (0, &[0xc3])]).is_err());
        assert!(package(&[(1_000_000, &[0xc3])]).is_err());
    }

    #[test]
    fn wrong_machine_truncation_and_mutated_section_extent_reject() {
        let mut object = package(&[(0, &[0xc3, 0xff])]).unwrap();
        assert!(functions(&object[..63], 1).is_err());
        object[18] = 183;
        assert!(functions(&object, 1).is_err());
        object[18] = 62;
        let table = u64::from_le_bytes(object[40..48].try_into().unwrap()) as usize;
        object[table + 64 + 32..table + 64 + 40].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(functions(&object, 1).is_err());
    }
}

use std::collections::{BTreeMap, BTreeSet};
const MAX_OBJECT: usize = 64 * 1024 * 1024;
const MAX_IMAGE: usize = 16 * 1024 * 1024;
const MAX_IMAGES: usize = 4096;

pub struct Function<'a> {
    pub address: u64,
    pub bytes: &'a [u8],
}

fn align(bytes: &mut Vec<u8>, alignment: usize) {
    bytes.resize(bytes.len().next_multiple_of(alignment), 0);
}
fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, String> {
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or("truncated ELF")?
            .try_into()
            .unwrap(),
    ))
}
fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or("truncated ELF")?
            .try_into()
            .unwrap(),
    ))
}
fn u64_at(bytes: &[u8], offset: usize) -> Result<u64, String> {
    Ok(u64::from_le_bytes(
        bytes
            .get(offset..offset + 8)
            .ok_or("truncated ELF")?
            .try_into()
            .unwrap(),
    ))
}
fn set16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}
fn set32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn set64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
fn slice(bytes: &[u8], offset: u64, length: u64) -> Result<&[u8], String> {
    let end = offset.checked_add(length).ok_or("ELF range overflow")?;
    let start = usize::try_from(offset).map_err(|_| "ELF offset overflow")?;
    let end = usize::try_from(end).map_err(|_| "ELF end overflow")?;
    bytes
        .get(start..end)
        .ok_or_else(|| "ELF range outside file".into())
}
fn name(strings: &[u8], offset: u32) -> Result<&str, String> {
    let tail = strings.get(offset as usize..).ok_or("ELF string offset")?;
    let end = tail
        .iter()
        .position(|byte| *byte == 0)
        .ok_or("unterminated ELF string")?;
    std::str::from_utf8(&tail[..end]).map_err(|_| "non-UTF8 ELF name".into())
}

/// One ET_REL object, one RX section, no relocations/imports/init code. The
/// complete image includes its constants; all relative displacements stay fixed.
pub fn package(images: &[(usize, &[u8])]) -> Result<Vec<u8>, String> {
    if images.is_empty() || images.len() > MAX_IMAGES {
        return Err("cached ELF image count".into());
    }
    let mut seen = BTreeSet::new();
    let mut text = Vec::new();
    let mut strings = vec![0];
    let mut symbols = vec![0; 24];
    for (index, image) in images {
        if *index >= 1_000_000
            || !seen.insert(*index)
            || image.is_empty()
            || image.len() > MAX_IMAGE
        {
            return Err("cached ELF image identity/length".into());
        }
        align(&mut text, 64);
        if text
            .len()
            .checked_add(image.len())
            .is_none_or(|end| end > MAX_OBJECT - 1024 * 1024)
        {
            return Err("cached ELF total length".into());
        }
        let offset = text.len();
        text.extend_from_slice(image);
        let string_offset = u32::try_from(strings.len()).map_err(|_| "ELF string size")?;
        strings.extend_from_slice(format!("op_impl_{index}\0").as_bytes());
        let mut symbol = [0; 24];
        set32(&mut symbol, 0, string_offset);
        symbol[4] = 0x12;
        set16(&mut symbol, 6, 1);
        set64(&mut symbol, 8, offset as u64);
        set64(&mut symbol, 16, image.len() as u64);
        symbols.extend_from_slice(&symbol);
    }
    let section_names = b"\0.text.cached\0.symtab\0.strtab\0.shstrtab\0.note.GNU-stack\0";
    let mut output = vec![0; 64];
    let mut sections = vec![[0_u8; 64]];
    for (label, kind, flags, data, link, info, alignment, entry) in [
        (1, 1, 6, text.as_slice(), 0, 0, 64, 0),
        (14, 2, 0, symbols.as_slice(), 3, 1, 8, 24),
        (22, 3, 0, strings.as_slice(), 0, 0, 1, 0),
        (30, 3, 0, section_names.as_slice(), 0, 0, 1, 0),
        (40, 1, 0, &[][..], 0, 0, 1, 0),
    ] {
        align(&mut output, alignment);
        let start = output.len();
        output.extend_from_slice(data);
        let mut section = [0; 64];
        set32(&mut section, 0, label);
        set32(&mut section, 4, kind);
        set64(&mut section, 8, flags);
        set64(&mut section, 24, start as u64);
        set64(&mut section, 32, data.len() as u64);
        set32(&mut section, 40, link);
        set32(&mut section, 44, info);
        set64(&mut section, 48, alignment as u64);
        set64(&mut section, 56, entry);
        sections.push(section);
    }
    align(&mut output, 8);
    let table = output.len();
    for section in sections {
        output.extend_from_slice(&section);
    }
    output[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    set16(&mut output, 16, 1);
    set16(&mut output, 18, 62);
    set32(&mut output, 20, 1);
    set64(&mut output, 40, table as u64);
    set16(&mut output, 52, 64);
    set16(&mut output, 58, 64);
    set16(&mut output, 60, 6);
    set16(&mut output, 62, 4);
    if output.len() > MAX_OBJECT {
        return Err("cached ELF object length".into());
    }
    Ok(output)
}

// These formats encode relocation addresses differently from ELF REL/RELA.
// The native container/link recipe does not produce them. Do not silently skip
// them: a dynamic loader could otherwise mutate admitted image bytes at dlopen.
// Values follow llvm/include/llvm/BinaryFormat/ELF.h (RELR/CREL/Android/auth RELR).
fn packed_relocations(kind: u32) -> bool {
    matches!(
        kind,
        19 | 0x4000_0014 | 0x6000_0001 | 0x6000_0002 | 0x6fff_ff00 | 0x7000_0004
    )
}

/// Read complete op_impl symbols from ET_REL (builder) or ET_DYN (loader).
/// Validate every cached symbol and reject relocations that could mutate it at
/// load. Ordinary wrapper/CRT relocations outside cached images remain separate.
pub fn functions(
    bytes: &[u8],
    expected_type: u16,
) -> Result<BTreeMap<String, Function<'_>>, String> {
    if bytes.len() < 64
        || bytes.len() > 512 * 1024 * 1024
        || &bytes[..7] != b"\x7fELF\x02\x01\x01"
        || u16_at(bytes, 16)? != expected_type
        || !matches!(expected_type, 1 | 3)
        || u16_at(bytes, 18)? != 62
        || u32_at(bytes, 20)? != 1
        || u16_at(bytes, 58)? != 64
    {
        return Err("cached ELF header/architecture".into());
    }
    let table = u64_at(bytes, 40)?;
    let count = usize::from(u16_at(bytes, 60)?);
    if !(1..=1024).contains(&count) {
        return Err("cached ELF section count".into());
    }
    let raw = slice(bytes, table, (count * 64) as u64)?;
    let sections: Vec<_> = raw.chunks_exact(64).collect();
    for section in &sections {
        if packed_relocations(u32_at(section, 4)?) {
            return Err("unadmitted packed cached-library relocation format".into());
        }
        if u32_at(section, 4)? != 8 {
            slice(bytes, u64_at(section, 24)?, u64_at(section, 32)?)?;
        }
    }
    let mut result: BTreeMap<String, Function<'_>> = BTreeMap::new();
    for symbol_table in &sections {
        if !matches!(u32_at(symbol_table, 4)?, 2 | 11) {
            continue;
        }
        let linked = sections
            .get(u32_at(symbol_table, 40)? as usize)
            .ok_or("cached ELF symbol strings")?;
        if u32_at(linked, 4)? != 3
            || u64_at(symbol_table, 56)? != 24
            || !u64_at(symbol_table, 32)?.is_multiple_of(24)
        {
            return Err("cached ELF symbol layout".into());
        }
        let strings = slice(bytes, u64_at(linked, 24)?, u64_at(linked, 32)?)?;
        let entries = slice(bytes, u64_at(symbol_table, 24)?, u64_at(symbol_table, 32)?)?;
        if entries.len() / 24 > 1_000_000 {
            return Err("cached ELF symbol count".into());
        }
        for symbol in entries.chunks_exact(24) {
            let label = name(strings, u32_at(symbol, 0)?)?;
            if !label.starts_with("op_impl_") {
                continue;
            }
            if symbol[4] != 0x12 || symbol[5] != 0 {
                return Err("cached ELF symbol type/binding".into());
            }
            let section = sections
                .get(usize::from(u16_at(symbol, 6)?))
                .ok_or("cached ELF symbol section")?;
            if u32_at(section, 4)? != 1 || u64_at(section, 8)? & 7 != 6 {
                return Err("cached ELF symbol is not RX".into());
            }
            let address = u64_at(symbol, 8)?;
            let length = u64_at(symbol, 16)?;
            if length == 0 || length > MAX_IMAGE as u64 {
                return Err("cached ELF symbol length".into());
            }
            let relative = address
                .checked_sub(u64_at(section, 16)?)
                .ok_or("cached ELF symbol address")?;
            let contents = slice(bytes, u64_at(section, 24)?, u64_at(section, 32)?)?;
            let function = Function {
                address,
                bytes: slice(contents, relative, length)?,
            };
            if let Some(previous) = result.get(label) {
                if previous.address != function.address || previous.bytes != function.bytes {
                    return Err("cached ELF duplicate symbol differs".into());
                }
            } else {
                result.insert(label.to_owned(), function);
            }
            if result.len() > MAX_IMAGES {
                return Err("cached ELF image count".into());
            }
        }
    }
    let mut ranges: Vec<_> = result
        .values()
        .map(|function| {
            (
                function.address,
                function.address.saturating_add(function.bytes.len() as u64),
            )
        })
        .collect();
    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err("cached ELF function ranges overlap".into());
    }
    let mut relocations = 0_usize;
    for section in &sections {
        let kind = u32_at(section, 4)?;
        if !matches!(kind, 4 | 9) || u64_at(section, 32)? == 0 {
            continue;
        }
        let width = if kind == 4 { 24 } else { 16 };
        if expected_type == 1
            || u64_at(section, 56)? != width
            || !u64_at(section, 32)?.is_multiple_of(width)
        {
            return Err("cached ELF relocation layout".into());
        }
        for relocation in
            slice(bytes, u64_at(section, 24)?, u64_at(section, 32)?)?.chunks_exact(width as usize)
        {
            relocations += 1;
            if relocations > 1_000_000 {
                return Err("cached ELF relocation count".into());
            }
            let relocation_type = u64_at(relocation, 8)? as u32;
            if !matches!(relocation_type, 1 | 2 | 6 | 7 | 8) {
                return Err("unadmitted cached-library relocation kind".into());
            }
            let address = u64_at(relocation, 0)?;
            let limit = address.saturating_add(16);
            let first = ranges.partition_point(|&(_, end)| end <= address);
            if ranges.get(first).is_some_and(|&(start, _)| start < limit) {
                return Err("relocation mutates a cached image".into());
            }
        }
    }
    if result.is_empty() {
        return Err("cached ELF has no image symbols".into());
    }
    Ok(result)
}

pub struct Reconstruction {
    pub image: Vec<u8>,
    pub text_bytes: usize,
    pub alignment: usize,
}

/// Recover the original tinygrad JIT layout from an independently authenticated
/// LLVM object, with no compiler, linker, provider import or generated-code load.
/// Only one entry-at-zero RX section, readonly data and internal PC32 fixups are
/// admitted. The caller must compare the ENTIRE result with the frozen BINARY.
pub fn reconstruct(bytes: &[u8]) -> Result<Reconstruction, String> {
    if bytes.len() < 64
        || bytes.len() > MAX_IMAGE
        || &bytes[..7] != b"\x7fELF\x02\x01\x01"
        || u16_at(bytes, 16)? != 1
        || u16_at(bytes, 18)? != 62
        || u16_at(bytes, 58)? != 64
    {
        return Err("cached reconstruction ELF header".into());
    }
    let count = usize::from(u16_at(bytes, 60)?);
    if !(1..=256).contains(&count) {
        return Err("cached reconstruction section count".into());
    }
    let raw = slice(bytes, u64_at(bytes, 40)?, (count * 64) as u64)?;
    let sections: Vec<_> = raw.chunks_exact(64).collect();
    let mut offsets = vec![None; count];
    let mut image = Vec::new();
    let mut executable = None;
    let mut alignment = 1;
    for (index, section) in sections.iter().enumerate() {
        let kind = u32_at(section, 4)?;
        let length = u64_at(section, 32)?;
        if packed_relocations(kind) {
            return Err("unadmitted packed cached-object relocation format".into());
        }
        if (kind == 9 || kind == 8) && length != 0 {
            return Err("cached reconstruction REL/BSS is unsupported".into());
        }
        if kind != 1 {
            continue;
        }
        let section_align = usize::try_from(u64_at(section, 48)?)
            .map_err(|_| "cached alignment overflow")?
            .max(1);
        if !section_align.is_power_of_two()
            || section_align > 64
            || u64_at(section, 16)? != 0
            || u64_at(section, 8)? & 1 != 0
        {
            return Err("cached reconstruction writable/fixed/alignment layout".into());
        }
        alignment = alignment.max(section_align);
        align(&mut image, section_align);
        let data = slice(bytes, u64_at(section, 24)?, length)?;
        let start = image.len();
        if start
            .checked_add(data.len())
            .is_none_or(|end| end > MAX_IMAGE)
        {
            return Err("cached reconstruction image bound".into());
        }
        offsets[index] = Some(start);
        image.extend_from_slice(data);
        if u64_at(section, 8)? & 4 != 0 {
            if executable.replace((index, start, data.len())).is_some() {
                return Err("cached reconstruction multiple executable sections".into());
            }
        }
    }
    let (text_index, text_offset, text_bytes) =
        executable.ok_or("cached reconstruction has no executable section")?;
    if text_offset != 0 || text_bytes == 0 {
        return Err("cached reconstruction entry is not offset zero".into());
    }
    let mut functions = 0;
    for section in &sections {
        if u32_at(section, 4)? != 2 {
            continue;
        }
        if u64_at(section, 56)? != 24 || !u64_at(section, 32)?.is_multiple_of(24) {
            return Err("cached reconstruction symbol table".into());
        }
        for symbol in slice(bytes, u64_at(section, 24)?, u64_at(section, 32)?)?.chunks_exact(24) {
            if symbol[4] & 15 != 2 {
                continue;
            }
            if symbol[4] >> 4 != 1
                || usize::from(u16_at(symbol, 6)?) != text_index
                || u64_at(symbol, 8)? != 0
                || u64_at(symbol, 16)? == 0
                || u64_at(symbol, 16)? > text_bytes as u64
            {
                return Err("cached reconstruction entry ABI/layout".into());
            }
            functions += 1;
        }
    }
    if functions != 1 {
        return Err("cached reconstruction requires one function entry".into());
    }
    let mut patched = BTreeSet::new();
    let mut relocations = 0;
    for section in &sections {
        if u32_at(section, 4)? != 4 {
            continue;
        }
        if u64_at(section, 56)? != 24 || !u64_at(section, 32)?.is_multiple_of(24) {
            return Err("cached reconstruction RELA layout".into());
        }
        let target_index = u32_at(section, 44)? as usize;
        let target = sections
            .get(target_index)
            .ok_or("cached relocation section index")?;
        let start = offsets
            .get(target_index)
            .copied()
            .flatten()
            .ok_or("cached relocation target is not PROGBITS")?;
        let symbols = sections
            .get(u32_at(section, 40)? as usize)
            .ok_or("cached relocation symbol table index")?;
        if u32_at(symbols, 4)? != 2 || u64_at(symbols, 56)? != 24 {
            return Err("cached relocation symbol table".into());
        }
        let symbols = slice(bytes, u64_at(symbols, 24)?, u64_at(symbols, 32)?)?;
        for entry in slice(bytes, u64_at(section, 24)?, u64_at(section, 32)?)?.chunks_exact(24) {
            relocations += 1;
            if relocations > 1_000_000 {
                return Err("cached reconstruction relocation count".into());
            }
            let offset = u64_at(entry, 0)?;
            let info = u64_at(entry, 8)?;
            let addend = i64::from_le_bytes(entry[16..24].try_into().unwrap());
            if info as u32 != 2
                || offset
                    .checked_add(4)
                    .is_none_or(|end| end > u64_at(target, 32).unwrap_or(0))
            {
                return Err("cached reconstruction requires bounded PC32".into());
            }
            let symbol = slice(
                symbols,
                (info >> 32)
                    .checked_mul(24)
                    .ok_or("cached symbol index overflow")?,
                24,
            )?;
            let symbol_section = usize::from(u16_at(symbol, 6)?);
            let symbol_base = offsets
                .get(symbol_section)
                .copied()
                .flatten()
                .ok_or("cached external/non-image symbol relocation")?;
            let section = sections
                .get(symbol_section)
                .ok_or("cached symbol section")?;
            if u64_at(symbol, 8)? > u64_at(section, 32)? {
                return Err("cached symbol value outside section".into());
            }
            let place = start
                .checked_add(usize::try_from(offset).map_err(|_| "cached relocation offset")?)
                .ok_or("cached relocation place overflow")?;
            for byte in place..place + 4 {
                if !patched.insert(byte) {
                    return Err("cached overlapping relocations".into());
                }
            }
            let target = symbol_base as i128 + i128::from(u64_at(symbol, 8)?) + i128::from(addend);
            if target + 4 < 0 || target + 4 >= image.len() as i128 {
                return Err("cached PC32 target outside complete image".into());
            }
            let value = i32::try_from(target - place as i128)
                .map_err(|_| "cached PC32 displacement overflow")?;
            image
                .get_mut(place..place + 4)
                .ok_or("cached PC32 place outside image")?
                .copy_from_slice(&value.to_le_bytes());
        }
    }
    Ok(Reconstruction {
        image,
        text_bytes,
        alignment,
    })
}

#[cfg(test)]
mod reconstruction_tests {
    use super::*;
    fn relocatable() -> Vec<u8> {
        let mut object = package(&[(
            0,
            &[0x8b, 0x05, 0, 0, 0, 0, 0xc3, 0, 0x12, 0x34, 0x56, 0x78],
        )])
        .unwrap();
        let table = u64_at(&object, 40).unwrap() as usize;
        let sections = object[table..table + 6 * 64].to_vec();
        align(&mut object, 8);
        let relocation = object.len();
        object.extend_from_slice(&[0; 24]);
        set64(&mut object, relocation, 2);
        set64(&mut object, relocation + 8, (1 << 32) | 2);
        set64(&mut object, relocation + 16, 4);
        let mut section = [0; 64];
        set32(&mut section, 4, 4);
        set64(&mut section, 24, relocation as u64);
        set64(&mut section, 32, 24);
        set32(&mut section, 40, 2);
        set32(&mut section, 44, 1);
        set64(&mut section, 48, 8);
        set64(&mut section, 56, 24);
        align(&mut object, 8);
        let table = object.len();
        object.extend_from_slice(&sections);
        object.extend_from_slice(&section);
        set64(&mut object, 40, table as u64);
        set16(&mut object, 60, 7);
        object
    }
    #[test]
    fn internal_pc32_reconstruction_preserves_complete_data_tail() {
        let result = reconstruct(&relocatable()).unwrap();
        assert_eq!(
            result.image,
            &[0x8b, 0x05, 2, 0, 0, 0, 0xc3, 0, 0x12, 0x34, 0x56, 0x78]
        );
        assert_eq!(result.alignment, 64);
    }
    #[test]
    fn external_symbol_and_non_pc32_relocations_are_not_certificates() {
        let mut object = relocatable();
        let table = u64_at(&object, 40).unwrap() as usize;
        let relocation = u64_at(&object[table + 6 * 64..], 24).unwrap() as usize;
        set64(&mut object, relocation + 8, (1 << 32) | 8);
        assert!(reconstruct(&object).is_err());
        set64(&mut object, relocation + 8, (1 << 32) | 2);
        let symbol_table = u64_at(&object[table + 2 * 64..], 24).unwrap() as usize;
        set16(&mut object, symbol_table + 24 + 6, 0);
        assert!(reconstruct(&object).is_err());
    }
}
