use serde::Serialize;

pub const DLC_LENGTHS: [usize; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 20, 24, 32, 48, 64];
pub const HEADER_SIZE: usize = 6;
pub const RECEIVE_SIZE: usize = 0x4000;
pub const TX_SOFT_LIMIT: usize = 0x100;
pub const BUS_OFFSET: u32 = 4;

#[derive(Debug, thiserror::Error)]
pub enum EncodeError<E> {
    #[error("unsupported CAN payload length {0}")]
    Length(usize),
    #[error("CAN transport write failed: {0}")]
    Write(E),
}

pub struct Encoder {
    bus_offset: u32,
    buffer: [u8; TX_SOFT_LIMIT * 2],
    used: usize,
}

impl Encoder {
    pub fn new(bus_offset: u32) -> Self {
        Self {
            bus_offset,
            buffer: [0; TX_SOFT_LIMIT * 2],
            used: 0,
        }
    }

    pub fn push<E>(
        &mut self,
        address: u32,
        source: u8,
        data: &[u8],
        write: &mut impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<(), EncodeError<E>> {
        let source = u32::from(source);
        if source < self.bus_offset || source >= self.bus_offset.wrapping_add(BUS_OFFSET) {
            return Ok(());
        }
        let code = DLC_LENGTHS
            .iter()
            .position(|length| *length == data.len())
            .ok_or(EncodeError::Length(data.len()))?;
        let position = self.used;
        self.buffer[position] = ((code as u8) << 4) | (((source - self.bus_offset) as u8) << 1);
        let address_flags = (address << 3) | (u32::from(address >= 0x800) << 2);
        self.buffer[position + 1..position + 5].copy_from_slice(&address_flags.to_le_bytes());
        self.buffer[position + 5] = 0;
        self.used += HEADER_SIZE + data.len();
        self.buffer[position + HEADER_SIZE..self.used].copy_from_slice(data);
        self.buffer[position + 5] = checksum(&self.buffer[position..self.used]);
        if self.used >= TX_SOFT_LIMIT {
            self.finish(write)?;
        }
        Ok(())
    }

    pub fn finish<E>(
        &mut self,
        write: &mut impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<(), EncodeError<E>> {
        if self.used != 0 {
            write(&self.buffer[..self.used]).map_err(EncodeError::Write)?;
            self.used = 0;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Frame {
    pub address: u32,
    pub src: u64,
    pub data: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
#[error("CAN receive buffer capacity exceeded")]
pub struct ReceiveCapacity;

pub struct Decoder {
    bus_offset: u32,
    buffer: [u8; RECEIVE_SIZE + HEADER_SIZE + 64],
    used: usize,
}

impl Decoder {
    pub fn new(bus_offset: u32) -> Self {
        Self {
            bus_offset,
            buffer: [0; RECEIVE_SIZE + HEADER_SIZE + 64],
            used: 0,
        }
    }

    pub fn remaining(&self) -> &[u8] {
        &self.buffer[..self.used]
    }

    pub fn push(&mut self, bytes: &[u8], frames: &mut Vec<Frame>) -> Result<bool, ReceiveCapacity> {
        if bytes.len() > RECEIVE_SIZE || bytes.len() > self.buffer.len() - self.used {
            return Err(ReceiveCapacity);
        }
        self.buffer[self.used..self.used + bytes.len()].copy_from_slice(bytes);
        self.used += bytes.len();
        let mut position = 0;
        while self.used - position >= HEADER_SIZE {
            let length = DLC_LENGTHS[usize::from(self.buffer[position] >> 4)];
            let end = position + HEADER_SIZE + length;
            if end > self.used {
                break;
            }
            if checksum(&self.buffer[position..end]) != 0 {
                self.used = 0;
                return Ok(false);
            }
            let flags = u32::from_le_bytes([
                self.buffer[position + 1],
                self.buffer[position + 2],
                self.buffer[position + 3],
                self.buffer[position + 4],
            ]);
            let bus = u32::from((self.buffer[position] >> 1) & 7);
            let mut source = u64::from(bus.wrapping_add(self.bus_offset));
            if flags & 1 != 0 {
                source += 0xc0;
            }
            if flags & 2 != 0 {
                source += 0x80;
            }
            frames.push(Frame {
                address: flags >> 3,
                src: source,
                data: self.buffer[position + HEADER_SIZE..end].to_vec(),
            });
            position = end;
        }
        self.buffer.copy_within(position..self.used, 0);
        self.used -= position;
        Ok(true)
    }
}

pub fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0, |sum, byte| sum ^ byte)
}
