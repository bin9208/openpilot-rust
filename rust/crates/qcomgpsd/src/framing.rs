use crate::Error;
pub const LOG_TYPES: [u16; 5] = [0x1477, 0x1480, 0x14de, 0x1476, 0x14e1];
pub const DIAG_LOG: u8 = 16;
pub const LOG_CONFIG: u8 = 115;
fn crc_byte(mut crc: u16, byte: u8) -> u16 {
    crc ^= u16::from(byte);
    for _ in 0..8 {
        crc = if crc & 1 != 0 {
            (crc >> 1) ^ 0x8408
        } else {
            crc >> 1
        };
    }
    crc
}
pub fn crc16(bytes: &[u8]) -> u16 {
    bytes.iter().fold(0xffff, |crc, &byte| crc_byte(crc, byte)) ^ 0xffff
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
// Log::parse admits at most u16::MAX bytes after its three-byte outer
// header. Include the opcode here, but not the two CRC bytes. This is a
// retention bound, NOT a maximum HDLC frame length: setup skips arbitrary
// DIAG_LOG frames and consumes only fixed prefixes of non-log replies.
const RETAINED_MESSAGE: usize = 1 + 3 + u16::MAX as usize;
const CONFIG_REPLY_PREFIX: usize = 3 + 4 + 4 + 16 * 4;

pub(crate) struct Message {
    opcode: u8,
    prefix: Vec<u8>,
    truncated: bool,
}
impl Message {
    pub(crate) fn full(self) -> Result<(u8, Vec<u8>), Error> {
        if self.truncated {
            return Err(Error::Protocol("diagnostic frame exceeds retained payload"));
        }
        Ok((self.opcode, self.prefix))
    }
    pub(crate) fn log(self) -> Result<(u8, Vec<u8>), Error> {
        if self.opcode == DIAG_LOG {
            if self.truncated {
                // A body larger than every u16 extent cannot pass Log::parse.
                return Err(Error::Protocol("diagnostic log extent"));
            }
            self.full()
        } else {
            // The steady-state consumer logs only the opcode.
            Ok((self.opcode, Vec::new()))
        }
    }
    pub(crate) fn reply(mut self) -> Option<(u8, Vec<u8>)> {
        if self.opcode == DIAG_LOG {
            return None;
        }
        // setup::logs needs at most the complete range response; NV and
        // subsystem response bodies are ignored. CRC covers every tail byte.
        self.prefix.truncate(CONFIG_REPLY_PREFIX);
        Some((self.opcode, self.prefix))
    }
}

pub struct Frames {
    pending: Vec<u8>,
    crc: u16,
    tail: [u8; 2],
    tail_len: usize,
    escaped: bool,
    // Saturates at two: only needed for the minimum raw envelope length.
    encoded_len: u8,
    truncated: bool,
    ready: std::collections::VecDeque<Result<Message, Error>>,
}
impl Default for Frames {
    fn default() -> Self {
        Self {
            pending: Vec::new(),
            crc: 0xffff,
            tail: [0; 2],
            tail_len: 0,
            escaped: false,
            encoded_len: 0,
            truncated: false,
            ready: std::collections::VecDeque::new(),
        }
    }
}
impl Frames {
    // Serial recv drains completed messages before adding its next 0x10000-byte
    // transfer. Only the unfinished message persists across transfers. Process
    // bytes now so delimiter-free data is never copied into an unbounded queue.
    pub fn extend(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if byte == 0x7e {
                if self.escaped {
                    self.decoded(0x7d);
                }
                let message = self.finish();
                self.ready.push_back(message);
                continue;
            }
            self.encoded_len = (self.encoded_len + 1).min(2);
            if self.escaped {
                self.escaped = false;
                if matches!(byte, 0x5d | 0x5e) {
                    self.decoded(byte ^ 0x20);
                    continue;
                }
                self.decoded(0x7d);
            }
            if byte == 0x7d {
                self.escaped = true;
            } else {
                self.decoded(byte);
            }
        }
    }
    fn decoded(&mut self, byte: u8) {
        if self.tail_len < 2 {
            self.tail[self.tail_len] = byte;
            self.tail_len += 1;
            return;
        }
        // Delay two decoded bytes so CRC bytes never enter the retained body.
        let payload_byte = self.tail[0];
        self.crc = crc_byte(self.crc, payload_byte);
        if self.pending.len() < RETAINED_MESSAGE {
            self.pending.push(payload_byte);
        } else {
            self.truncated = true;
        }
        self.tail = [self.tail[1], byte];
    }
    fn finish(&mut self) -> Result<Message, Error> {
        // Preserve whole-frame error precedence even after retention overflow.
        let result = if self.encoded_len < 2 {
            Err(Error::Protocol("invalid HDLC envelope"))
        } else if self.tail_len < 2 {
            Err(Error::Protocol("missing HDLC checksum"))
        } else if self.tail != (self.crc ^ 0xffff).to_le_bytes() {
            Err(Error::Protocol("HDLC checksum mismatch"))
        } else if self.pending.is_empty() {
            Err(Error::Protocol("empty diagnostic message"))
        } else {
            let mut prefix = std::mem::take(&mut self.pending);
            let opcode = prefix.remove(0);
            Ok(Message {
                opcode,
                prefix,
                truncated: self.truncated,
            })
        };
        self.pending.clear();
        self.crc = 0xffff;
        self.tail_len = 0;
        self.escaped = false;
        self.encoded_len = 0;
        self.truncated = false;
        result
    }
    pub(crate) fn next_message(&mut self) -> Result<Option<Message>, Error> {
        self.ready.pop_front().transpose()
    }
    /// Full-payload interface: oversized messages fail explicitly, never truncate.
    pub fn next_frame(&mut self) -> Result<Option<(u8, Vec<u8>)>, Error> {
        self.next_message()?.map(Message::full).transpose()
    }
}

#[cfg(test)]
#[path = "../tests/data/framing_retention.rs"]
mod retention_tests;
