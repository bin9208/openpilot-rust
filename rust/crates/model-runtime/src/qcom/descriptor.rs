use super::{invalid, ProgramImage};
use crate::Error;

fn word(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset.checked_add(4).ok_or_else(invalid)?)
            .ok_or_else(invalid)?
            .try_into()
            .map_err(|_| invalid())?,
    ))
}

fn round_up(value: u32, alignment: u32) -> Result<u32, Error> {
    Ok(value.checked_add(alignment - 1).ok_or_else(invalid)? / alignment * alignment)
}

impl ProgramImage {
    pub fn parse(name: &str, lib: &[u8]) -> Result<Self, Error> {
        if lib.len() > 64 * 1024 * 1024 || name.is_empty() || name.len() > 4096 || !name.is_ascii()
        {
            return Err(invalid());
        }
        let table = word(lib, 0x14)? as usize;
        let sections = word(lib, 0x18)? as usize;
        if table != 0x30 || !(12..=256).contains(&sections) {
            return Err(invalid());
        }
        for index in 0..sections {
            let entry = table + index * 20;
            let start = word(lib, entry + 4)? as usize;
            let size = word(lib, entry + 8)? as usize;
            if word(lib, entry)? as usize != index
                || start < table + sections * 20
                || start.checked_add(size).ok_or_else(invalid)? > lib.len()
            {
                return Err(invalid());
            }
        }
        let image_size = word(lib, 0x100)?;
        let image_offset = word(lib, 0xc0)? as usize;
        if image_size == 0 || image_size % 4 != 0 || image_size.div_ceil(128) > 1023 {
            return Err(invalid());
        }
        let image = lib
            .get(
                image_offset
                    ..image_offset
                        .checked_add(image_size as usize)
                        .ok_or_else(invalid)?,
            )
            .ok_or_else(invalid)?
            .to_vec();
        let desc = word(lib, 0x110)? as usize;
        let desc_end = desc
            .checked_add(word(lib, 0x114)? as usize)
            .ok_or_else(invalid)?;
        let name_start = desc.checked_add(0x158).ok_or_else(invalid)?;
        let name_end = name_start.checked_add(name.len()).ok_or_else(invalid)?;
        if desc_end > lib.len()
            || name_end > desc_end
            || lib.get(name_start..name_end) != Some(name.as_bytes())
        {
            return Err(invalid());
        }
        let read_desc = |offset| word(lib, desc.checked_add(offset).ok_or_else(invalid)?);
        let prg_offset = read_desc(0xc4)?;
        let brnchstck = read_desc(0x108)? / 2;
        let pvtmem = read_desc(0xc8)?;
        let shmem = read_desc(0xd8)?;
        let file_samplers = read_desc(0xdc)?;
        if file_samplers > 1 || prg_offset >= image_size || brnchstck > 63 {
            return Err(invalid());
        }
        let samp_cnt = file_samplers * 2;
        let samplers = if samp_cnt == 0 {
            vec![]
        } else {
            vec![7008, 48, 0, 0, 0, 0, 0, 0]
        };
        let mut offset = round_up(
            u32::try_from(desc + 0x158 + name.len()).map_err(|_| invalid())?,
            4,
        )? as usize
            + 8 * file_samplers as usize;
        let mut buf_offs = Vec::new();
        let (mut tex_cnt, mut ibo_cnt) = (0, 0);
        while offset.checked_add(32).ok_or_else(invalid)? <= desc_end {
            let length = word(lib, offset)? as usize;
            if length == 0 {
                break;
            }
            if length < 32
                || !length.is_multiple_of(4)
                || offset.checked_add(length).ok_or_else(invalid)? > desc_end
            {
                return Err(invalid());
            }
            let location = word(lib, offset + 12)?.checked_mul(4).ok_or_else(invalid)?;
            match word(lib, offset + 28)? {
                0 => {
                    if location > 2040 || location % 4 != 0 {
                        return Err(invalid());
                    }
                    buf_offs.push(location);
                }
                1 => tex_cnt += 1,
                2 => ibo_cnt += 1,
                _ => return Err(invalid()),
            }
            if buf_offs.len() > 256 || tex_cnt > 127 || ibo_cnt > 127 {
                return Err(invalid());
            }
            offset += length;
        }
        let mut consts_info = Vec::new();
        if word(lib, 0xb0)? != 0 {
            let mut offset = word(lib, 0xac)? as usize;
            if offset > image_offset {
                return Err(invalid());
            }
            while offset + 40 <= image_offset {
                let value = word(lib, offset)?;
                let is32 = word(lib, offset + 24)?;
                if is32 > 1 {
                    return Err(invalid());
                }
                let size = 2 << is32;
                let location = word(lib, offset + 16)?
                    .checked_mul(size)
                    .ok_or_else(invalid)?;
                if location.checked_add(size).ok_or_else(invalid)? > 2048
                    || (size == 2 && value > 65535)
                {
                    return Err(invalid());
                }
                consts_info.push((value, location, size));
                if consts_info.len() > 1024 {
                    return Err(invalid());
                }
                offset += 40;
            }
        }
        let registers = word(lib, 0x34)? as usize;
        let fregs = word(lib, registers.checked_add(0x14).ok_or_else(invalid)?)?;
        let hregs = word(lib, registers.checked_add(0x18).ok_or_else(invalid)?)?;
        if fregs > 63 || hregs > 63 {
            return Err(invalid());
        }
        let pvtmem_size_per_item = round_up(pvtmem, 512)? >> 9;
        if pvtmem_size_per_item > 255 {
            return Err(invalid());
        }
        let pvtmem_size_total = pvtmem_size_per_item * 128 * 2;
        let hw_stack_offset = round_up(
            round_up(pvtmem, 512)?
                .checked_next_power_of_two()
                .ok_or_else(invalid)?
                .checked_mul(128 * 16)
                .ok_or_else(invalid)?,
            4096,
        )?;
        let shared_size = shmem
            .saturating_sub(1)
            .checked_div(1024)
            .ok_or_else(invalid)?
            .max(1);
        if shared_size > 31 {
            return Err(invalid());
        }
        let max_threads =
            (384 * 32 / ((fregs + round_up(hregs, 2)? / 2).max(1) * 128) * 128).min(1024);
        if max_threads == 0 {
            return Err(invalid());
        }
        let ibo_off = 2048;
        let tex_off = ibo_off + 64 * ibo_cnt;
        let samp_off = tex_off + 64 * tex_cnt;
        let kernargs_alloc_size = round_up(samp_off + samplers.len() as u32 * 4, 256)?;
        Ok(Self {
            image_size,
            prg_offset,
            brnchstck,
            pvtmem,
            shmem,
            samp_cnt,
            samplers,
            buf_offs,
            tex_cnt,
            ibo_cnt,
            ibo_off,
            tex_off,
            samp_off,
            consts_info,
            fregs,
            hregs,
            pvtmem_size_per_item,
            pvtmem_size_total,
            hw_stack_offset,
            shared_size,
            max_threads,
            kernargs_alloc_size,
            image,
        })
    }

    pub fn image(&self) -> &[u8] {
        &self.image
    }
}
