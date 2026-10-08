use thiserror::Error;

#[derive(Debug, Error)]
#[error("CDM command exceeds the caller's destination buffer")]
pub struct PackingError;

#[derive(Clone, Copy, Debug)]
pub struct Dmi {
    pub length: u32,
    pub address: u32,
    pub selector: u8,
    pub opcode: u8,
}

pub fn write_dmi(destination: &mut [u8], command: Dmi) -> Result<usize, PackingError> {
    let output = destination.get_mut(..12).ok_or(PackingError)?;
    let length = command.length.wrapping_sub(1).to_le_bytes();
    output[..4].copy_from_slice(&[length[0], length[1], 0, command.opcode]);
    output[4..8].fill(0);
    let address = command.address.to_le_bytes();
    output[8..12].copy_from_slice(&[address[0], address[1], address[2], command.selector]);
    Ok(4)
}

pub fn write_cont(
    destination: &mut [u8],
    register: u32,
    values: &[u32],
) -> Result<usize, PackingError> {
    let size = values
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(8))
        .ok_or(PackingError)?;
    let output = destination.get_mut(..size).ok_or(PackingError)?;
    let count = values.len().to_le_bytes();
    output[..4].copy_from_slice(&[count[0], count[1], 0, 3]);
    let address = register.to_le_bytes();
    output[4..8].copy_from_slice(&[address[0], address[1], address[2], 0]);
    for (value, bytes) in values.iter().zip(output[8..].chunks_exact_mut(4)) {
        bytes.copy_from_slice(&value.to_le_bytes());
    }
    Ok(size)
}

pub fn write_random(destination: &mut [u8], values: &[u32]) -> Result<usize, PackingError> {
    let size = values
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(4))
        .ok_or(PackingError)?;
    let output = destination.get_mut(..size).ok_or(PackingError)?;
    let count = (values.len() / 2).to_le_bytes();
    output[..4].copy_from_slice(&[count[0], count[1], 0, 4]);
    for (value, bytes) in values.iter().zip(output[4..].chunks_exact_mut(4)) {
        bytes.copy_from_slice(&value.to_le_bytes());
    }
    Ok(size)
}
