use super::{
    manifest::{Blob, Expression, Space, View},
    Allocation, Device,
};
use crate::{hcq_vm::Memory, Error};
use sha2::{Digest, Sha256};
use std::io::{Read, Seek, SeekFrom};

#[derive(Clone, Copy)]
pub(super) enum Bound {
    Host { address: u64, bytes: u64 },
    Device(Allocation),
}
impl Bound {
    pub fn address(self, offset: u64, space: Space) -> Result<u64, Error> {
        let (base, bytes) = match self {
            Self::Host { address, bytes } => (address, bytes),
            Self::Device(value) => (
                match space {
                    Space::Host => value.host,
                    Space::Device => value.device,
                },
                value.bytes,
            ),
        };
        if offset > bytes {
            return Err(Error::Contract("HCQ view outside buffer"));
        }
        base.checked_add(offset)
            .ok_or(Error::Contract("HCQ buffer address overflow"))
    }
    pub fn write(
        self,
        offset: u64,
        data: &[u8],
        memory: &mut Memory,
        device: &mut impl Device,
    ) -> Result<(), Error> {
        let bytes = match self {
            Self::Host { bytes, .. } => bytes,
            Self::Device(value) => value.bytes,
        };
        if offset
            .checked_add(
                u64::try_from(data.len())
                    .map_err(|_| Error::Contract("HCQ write size overflow"))?,
            )
            .is_none_or(|end| end > bytes)
        {
            return Err(Error::Contract("HCQ model write outside buffer"));
        }
        match self {
            Self::Host { .. } => memory.write(self.address(offset, Space::Host)?, data),
            Self::Device(value) => device.write(value, offset, data),
        }
    }
}
pub(super) fn buffer(buffers: &[Bound], view: View) -> Result<Bound, Error> {
    buffers
        .get(view.buffer)
        .copied()
        .ok_or(Error::Contract("HCQ buffer index out of range"))
}
pub(super) fn expression(value: &Expression, buffers: &[Bound]) -> Result<u64, Error> {
    Ok(match value {
        Expression::Constant { value } => *value,
        Expression::Address { view, space } => {
            buffer(buffers, *view)?.address(view.offset, *space)?
        }
        Expression::Add { left, right } => expression(left, buffers)?
            .checked_add(expression(right, buffers)?)
            .ok_or(Error::Contract("HCQ link addition overflow"))?,
        Expression::Shr { left, right } => expression(left, buffers)?
            .checked_shr(
                u32::try_from(expression(right, buffers)?)
                    .map_err(|_| Error::Contract("HCQ link shift overflow"))?,
            )
            .ok_or(Error::Contract("HCQ link shift outside width"))?,
    })
}
pub(super) fn load_blob(
    file: &mut std::fs::File,
    blob: &Blob,
    mut write: impl FnMut(u64, &[u8]) -> Result<(), Error>,
) -> Result<(), Error> {
    if blob
        .offset
        .checked_add(blob.bytes)
        .is_none_or(|end| file.metadata().map_or(true, |m| end > m.len()))
    {
        return Err(Error::Contract("HCQ artifact blob outside file"));
    }
    file.seek(SeekFrom::Start(blob.offset))?;
    let mut bytes = vec![
        0;
        usize::try_from(blob.bytes.min(256 << 10))
            .map_err(|_| Error::Contract("HCQ blob size overflow"))?
    ];
    let mut offset = 0;
    let mut digest = Sha256::new();
    while offset < blob.bytes {
        let size = usize::try_from((blob.bytes - offset).min(bytes.len() as u64))
            .map_err(|_| Error::Contract("HCQ chunk size overflow"))?;
        file.read_exact(&mut bytes[..size])?;
        digest.update(&bytes[..size]);
        write(offset, &bytes[..size])?;
        offset += size as u64;
    }
    if format!("{:x}", digest.finalize()) != blob.sha256 {
        return Err(Error::Contract("HCQ artifact blob digest mismatch"));
    }
    Ok(())
}
pub(super) fn verify(file: &mut std::fs::File, bytes: u64, expected: &str) -> Result<(), Error> {
    if file.metadata()?.len() != bytes {
        return Err(Error::Contract("HCQ artifact size mismatch"));
    }
    let mut digest = Sha256::new();
    let mut buffer = vec![0; 256 << 10];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    if format!("{:x}", digest.finalize()) != expected {
        return Err(Error::Contract("HCQ artifact digest mismatch"));
    }
    Ok(())
}
