use crate::{
    checksum::{crc8, Kind},
    signal::Signal,
    Error,
};
mod constants {
    include!("volkswagen_constants.rs");
}

pub(crate) fn checksum(
    kind: Kind,
    address: u32,
    _signal: &Signal,
    data: &[u8],
) -> Result<u16, Error> {
    if kind == Kind::VolkswagenMlb
        && matches!(
            address,
            0x109 | 0x111 | 0x30c | 0x324 | 0x10b | 0x10d | 0x10f | 0x311 | 0x397 | 0x10c
        )
    {
        // Preserve the current source TypeError; fixing the vehicle CRC is a separate issue.
        return Err(Error::InheritedMlbChecksum);
    }
    let counter = usize::from(*data.get(1).ok_or(Error::Checksum)? & 15);
    if kind == Kind::VolkswagenGen2 {
        if let Some((length, constants)) =
            constants::GEN2
                .iter()
                .find_map(|(addr, length, constants)| {
                    (*addr == address).then_some((*length, constants))
                })
        {
            let crc = crc8(
                data.iter()
                    .take(length)
                    .skip(1)
                    .fold(255, |crc, b| crc8(crc ^ b, 0x2f))
                    ^ constants[counter],
                0x2f,
            ) ^ 255;
            if data.first() == Some(&crc) {
                return Ok(u16::from(crc));
            }
        }
    }
    let mut crc = data.iter().skip(1).fold(255, |crc, b| crc8(crc ^ b, 0x2f));
    if let Some((_, constants)) = constants::MQB.iter().find(|(addr, _)| *addr == address) {
        crc = crc8(crc ^ constants[counter], 0x2f);
    }
    Ok(u16::from(crc ^ 255))
}
