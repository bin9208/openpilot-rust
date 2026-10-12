// SPDX-License-Identifier: GPL-3.0-or-later
// Protocol derived from bundled turing-smart-screen-python/library/lcd/lcd_comm_turing_usb.py.
// Copyright (C) 2021 Matthieu Houdebine (mathoudebine).
use crate::Error;
use des::{
    cipher::{Block, BlockEncrypt, KeyInit},
    Des,
};
use num_traits::ToPrimitive;

const KEY: [u8; 8] = *b"slv3tuzx";

/// Preserve the source floating subtraction/multiplication before struct.pack('<I').
///
/// # Errors
/// Returns the source u32 range failure for a negative/nonfinite/overflowing value.
pub fn milliseconds(wall_time: f64, local_midnight: f64) -> Result<u32, Error> {
    ((wall_time - local_midnight) * 1000.0)
        .to_u32()
        .ok_or(Error::Contract("USB command timestamp out of range"))
}

/// Build and encrypt the original 500-byte command, zero-pad to 504, append trailer.
/// `milliseconds` is the original wall time minus local midnight, converted to u32.
///
/// # Errors
/// Rejects an out-of-bounds field like the original bytearray operation.
pub fn command(id: u8, milliseconds: u32, fields: &[(usize, i64)]) -> Result<[u8; 512], Error> {
    let mut plain = [0_u8; 504];
    plain[0] = id;
    plain[2] = 0x1a;
    plain[3] = 0x6d;
    plain[4..8].copy_from_slice(&milliseconds.to_le_bytes());
    for &(offset, value) in fields {
        let field = plain
            .get_mut(..500)
            .and_then(|bytes| bytes.get_mut(offset))
            .ok_or(Error::Contract("USB command field index out of range"))?;
        *field = u8::try_from(value & 0xff)
            .map_err(|_| Error::Contract("USB command field conversion failed"))?;
    }
    let cipher = Des::new(&KEY.into());
    let mut previous: Block<Des> = KEY.into();
    let mut output = [0_u8; 512];
    for (input, target) in plain.chunks_exact(8).zip(output[..504].chunks_exact_mut(8)) {
        let mut block = Block::<Des>::default();
        block.copy_from_slice(input);
        for (value, prior) in block.iter_mut().zip(previous.iter()) {
            *value ^= prior;
        }
        cipher.encrypt_block(&mut block);
        target.copy_from_slice(&block);
        previous = block;
    }
    output[510] = 161;
    output[511] = 26;
    Ok(output)
}

/// Source frame and H264 chunk size fields use big-endian u32; only last H264 sets byte12.
///
/// # Errors
/// Rejects a payload too large to fit the source size field/address space.
pub fn frame(id: u8, milliseconds: u32, bytes: &[u8], last: bool) -> Result<Vec<u8>, Error> {
    let size =
        u32::try_from(bytes.len()).map_err(|_| Error::Contract("USB frame size overflow"))?;
    let mut fields: Vec<(usize, i64)> = size
        .to_be_bytes()
        .iter()
        .enumerate()
        .map(|(index, value)| (8 + index, i64::from(*value)))
        .collect();
    if id == 121 && last {
        fields.push((12, 1));
    }
    let mut output = Vec::with_capacity(512 + bytes.len());
    output.extend(command(id, milliseconds, &fields)?);
    output.extend_from_slice(bytes);
    Ok(output)
}
