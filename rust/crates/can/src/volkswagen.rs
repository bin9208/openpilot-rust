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
    signal: &Signal,
    data: &[u8],
) -> Result<u16, Error> {
    if kind == Kind::VolkswagenMlb {
        // Source: commaai/opendbc c3955753, mlbcan.py seed table and mqbcan.py XOR.
        let initial: Option<u8> = match address {
            0x109 => Some(0x08),
            0x111 => Some(0x10),
            0x30c => Some(0x0f),
            0x324 => Some(0x27),
            0x10b => Some(0x0a),
            0x10d => Some(0x0c),
            0x10f => Some(0x0e),
            0x311 => Some(0x12),
            0x397 => Some(0x94),
            0x10c => Some(0x0d),
            _ => None,
        };
        if let Some(initial) = initial {
            let checksum = data
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != signal.start_bit / 8)
                .fold(initial, |checksum, (_, byte)| checksum ^ *byte);
            return Ok(u16::from(checksum));
        }
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
