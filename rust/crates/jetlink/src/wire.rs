//! Jetlink v2 framing and FunctionFS descriptors.
//! Ported from third_party/jetlink (MIT, Copyright (c) 2026 Zeph Leggett).
use crate::Error;
pub const HEADER_SIZE: usize = 32;
pub const TX_ALIGN: usize = 16384;
pub const PADDED: u32 = 128;
#[derive(Debug, PartialEq, Eq)]
pub struct Header {
    pub kind: u16,
    pub sequence: u32,
    pub flags: u32,
    pub length: u32,
    pub reserved: u64,
}
impl Header {
    pub fn encode(&self) -> [u8; 32] {
        let mut bytes = [0; 32];
        bytes[..4].copy_from_slice(b"JLNK");
        bytes[4..6].copy_from_slice(&2u16.to_le_bytes());
        bytes[6..8].copy_from_slice(&self.kind.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.sequence.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.flags.to_le_bytes());
        bytes[16..20].copy_from_slice(&self.length.to_le_bytes());
        bytes[20..28].copy_from_slice(&self.reserved.to_le_bytes());
        bytes
    }
    pub fn decode(bytes: &[u8; 32]) -> Result<Self, Error> {
        if &bytes[..4] != b"JLNK" || bytes[4..6] != 2u16.to_le_bytes() {
            return Err(Error::Contract("wire magic/version"));
        }
        Ok(Self {
            kind: u16::from_le_bytes([bytes[6], bytes[7]]),
            sequence: word(bytes, 8),
            flags: word(bytes, 12),
            length: word(bytes, 16),
            reserved: u64::from_le_bytes(
                bytes[20..28]
                    .try_into()
                    .map_err(|_| Error::Contract("wire header"))?,
            ),
        })
    }
}
pub fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}
pub fn frame(kind: u16, sequence: u32, payload: &[u8], gadget: bool) -> Result<Vec<u8>, Error> {
    let length = u32::try_from(payload.len()).map_err(|_| Error::Contract("wire length"))?;
    let total = HEADER_SIZE + payload.len();
    let pad = if gadget {
        (TX_ALIGN - total % TX_ALIGN) % TX_ALIGN
    } else {
        usize::from(total.is_multiple_of(1024))
    };
    let flags = if !gadget && pad != 0 { PADDED } else { 0 };
    let mut bytes = Header {
        kind,
        sequence,
        flags,
        length,
        reserved: 0,
    }
    .encode()
    .to_vec();
    bytes.extend(payload);
    bytes.resize(total + pad, 0);
    Ok(bytes)
}
pub fn descriptors() -> Vec<u8> {
    let mut body = Vec::new();
    for count in [3u32, 3, 5] {
        body.extend(count.to_le_bytes());
    }
    for packet in [64u16, 512, 1024] {
        body.extend([9, 4, 0, 0, 2, 255, 255, 255, 1]);
        for endpoint in [1, 130] {
            body.extend([7, 5, endpoint, 2]);
            body.extend(packet.to_le_bytes());
            body.push(0);
            if packet == 1024 {
                body.extend([6, 48, 15, 0, 0, 0]);
            }
        }
    }
    let mut bytes = Vec::new();
    for value in [3u32, 105, 7] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend(body);
    bytes
}
pub fn strings() -> Vec<u8> {
    let mut bytes = Vec::new();
    for value in [2u32, 26, 1, 1] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend(0x0409u16.to_le_bytes());
    bytes.extend(b"jetlink\0");
    bytes
}
