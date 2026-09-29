use super::{invalid, Argument, ProgramImage};
use crate::Error;

fn write(bytes: &mut [u8], offset: u32, data: &[u8]) -> Result<(), Error> {
    let start = offset as usize;
    bytes
        .get_mut(start..start.checked_add(data.len()).ok_or_else(invalid)?)
        .ok_or_else(invalid)?
        .copy_from_slice(data);
    Ok(())
}

impl ProgramImage {
    pub fn arguments(&self, arguments: &[Argument], scalars: &[u32]) -> Result<Vec<u8>, Error> {
        let buffers: Vec<_> = arguments
            .iter()
            .filter_map(|arg| match arg {
                Argument::Buffer { address } => Some(*address),
                _ => None,
            })
            .collect();
        let images: Vec<_> = arguments
            .iter()
            .filter(|arg| matches!(arg, Argument::Image { .. }))
            .collect();
        if buffers.len() + scalars.len() != self.buf_offs.len()
            || images.len() != (self.ibo_cnt + self.tex_cnt) as usize
        {
            return Err(invalid());
        }
        let mut bytes = vec![0; self.kernargs_alloc_size as usize];
        for &(value, offset, size) in &self.consts_info {
            write(&mut bytes, offset, &value.to_le_bytes()[..size as usize])?;
        }
        for (index, value) in self.samplers.iter().enumerate() {
            write(
                &mut bytes,
                self.samp_off + index as u32 * 4,
                &value.to_le_bytes(),
            )?;
        }
        for (index, address) in buffers.iter().enumerate() {
            write(&mut bytes, self.buf_offs[index], &address.to_le_bytes())?;
        }
        for (index, value) in scalars.iter().enumerate() {
            write(
                &mut bytes,
                self.buf_offs[buffers.len() + index],
                &value.to_le_bytes(),
            )?;
        }
        for (index, image) in images.iter().enumerate() {
            let Argument::Image {
                address,
                width,
                height,
                pitch,
                element_bytes,
            } = **image
            else {
                unreachable!()
            };
            let format = match element_bytes {
                4 => 0x82,
                2 => 0x62,
                _ => return Err(invalid()),
            };
            if width == 0
                || height == 0
                || width > 32767
                || height > 32767
                || pitch == 0
                || pitch % 64 != 0
                || pitch > 0x3fffff
                || pitch < width * 4 * element_bytes
                || pitch.trailing_zeros() > 21
            {
                return Err(invalid());
            }
            let texture = index >= self.ibo_cnt as usize;
            let words = [
                format << 22
                    | if texture {
                        8 | 1 << 7 | 2 << 10 | 3 << 13
                    } else {
                        0
                    },
                width | height << 15,
                1 << 29 | pitch << 7 | (pitch.trailing_zeros() - 6),
                0,
                address as u32,
                (address >> 32) as u32,
                0x40000000,
                13,
            ];
            for (word_index, value) in words.iter().enumerate() {
                write(
                    &mut bytes,
                    self.ibo_off + index as u32 * 64 + word_index as u32 * 4,
                    &value.to_le_bytes(),
                )?;
            }
        }
        Ok(bytes)
    }
}
