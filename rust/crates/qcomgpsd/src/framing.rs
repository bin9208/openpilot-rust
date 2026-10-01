use crate::Error;
pub const LOG_TYPES: [u16; 5] = [0x1477, 0x1480, 0x14de, 0x1476, 0x14e1];
pub const DIAG_LOG: u8 = 16;
pub const LOG_CONFIG: u8 = 115;
pub fn crc16(bytes: &[u8]) -> u16 {
    let mut crc = 0xffff_u16;
    for &byte in bytes {
        crc ^= u16::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0x8408
            } else {
                crc >> 1
            };
        }
    }
    crc ^ 0xffff
}
pub fn encode(payload: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(payload.len() + 3);
    for byte in payload.iter().copied().chain(crc16(payload).to_le_bytes()) {
        match byte {
            0x7d | 0x7e => output.extend([0x7d, byte ^ 0x20]),
            value => output.push(value),
        }
    }
    output.push(0x7e);
    output
}
pub fn decode(frame: &[u8]) -> Result<Vec<u8>, Error> {
    if frame.len() < 3 || frame.last() != Some(&0x7e) {
        return Err(Error::Protocol("invalid HDLC envelope"));
    }
    let mut bytes = Vec::with_capacity(frame.len() - 1);
    let mut source = frame[..frame.len() - 1].iter().copied().peekable();
    while let Some(byte) = source.next() {
        if byte == 0x7d
            && source
                .peek()
                .is_some_and(|value| matches!(*value, 0x5d | 0x5e))
        {
            bytes.push(source.next().ok_or(Error::Protocol("escape byte"))? ^ 0x20);
        } else {
            bytes.push(byte);
        }
    }
    if bytes.len() < 2 {
        return Err(Error::Protocol("missing HDLC checksum"));
    }
    let length = bytes.len() - 2;
    if bytes[length..] != crc16(&bytes[..length]).to_le_bytes() {
        return Err(Error::Protocol("HDLC checksum mismatch"));
    }
    bytes.truncate(length);
    Ok(bytes)
}
#[derive(Default)]
pub struct Frames {
    pending: Vec<u8>,
}
impl Frames {
    pub fn extend(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
    }
    pub fn next_frame(&mut self) -> Result<Option<(u8, Vec<u8>)>, Error> {
        let Some(end) = self.pending.iter().position(|byte| *byte == 0x7e) else {
            return Ok(None);
        };
        let frame: Vec<_> = self.pending.drain(..=end).collect();
        let mut payload = decode(&frame)?;
        if payload.is_empty() {
            return Err(Error::Protocol("empty diagnostic message"));
        }
        let opcode = payload.remove(0);
        Ok(Some((opcode, payload)))
    }
}
