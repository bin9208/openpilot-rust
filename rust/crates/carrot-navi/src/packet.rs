use crate::{json::Value, Error};

pub const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
pub const HEADER_SIZE: usize = 40;

#[derive(Debug, Clone, Copy)]
pub struct Metadata {
    pub protocol_version: u8,
    pub message_type: u8,
    pub format_or_reason: u8,
    pub flags: u8,
    pub stream_handle: u32,
    pub manifest_revision: u32,
    pub sequence: u64,
    pub source_timestamp_ms: u64,
    pub payload_length: u32,
    pub width: u16,
    pub height: u16,
}

impl Metadata {
    pub fn value(self) -> Value {
        Value::object([
            ("protocol_version", Value::integer(self.protocol_version)),
            ("message_type", Value::integer(self.message_type)),
            ("format_or_reason", Value::integer(self.format_or_reason)),
            ("flags", Value::integer(self.flags)),
            ("stream_handle", Value::integer(self.stream_handle)),
            ("manifest_revision", Value::integer(self.manifest_revision)),
            ("sequence", Value::integer(self.sequence)),
            (
                "source_timestamp_ms",
                Value::integer(self.source_timestamp_ms),
            ),
            ("payload_length", Value::integer(self.payload_length)),
            ("width", Value::integer(self.width)),
            ("height", Value::integer(self.height)),
        ])
    }
}

pub fn parse(packet: &[u8]) -> Result<(Metadata, &[u8]), Error> {
    if !(HEADER_SIZE..=MAX_MESSAGE_BYTES).contains(&packet.len()) {
        return Err(Error::value("invalid v2 binary packet size"));
    }
    let header = packet
        .get(..HEADER_SIZE)
        .ok_or_else(|| Error::value("invalid v2 binary packet size"))?;
    if header.get(..4) != Some(b"CNV2") || header.get(4) != Some(&2) {
        return Err(Error::value("unsupported v2 binary header"));
    }
    fn bytes<const N: usize>(header: &[u8], start: usize) -> Result<[u8; N], Error> {
        header
            .get(start..start + N)
            .and_then(|slice| slice.try_into().ok())
            .ok_or_else(|| Error::value("invalid v2 binary packet size"))
    }
    let payload = packet
        .get(HEADER_SIZE..)
        .ok_or_else(|| Error::value("invalid v2 binary packet size"))?;
    let metadata = Metadata {
        protocol_version: 2,
        message_type: bytes::<1>(header, 5)?[0],
        format_or_reason: bytes::<1>(header, 6)?[0],
        flags: bytes::<1>(header, 7)?[0],
        stream_handle: u32::from_be_bytes(bytes(header, 8)?),
        manifest_revision: u32::from_be_bytes(bytes(header, 12)?),
        sequence: u64::from_be_bytes(bytes(header, 16)?),
        source_timestamp_ms: u64::from_be_bytes(bytes(header, 24)?),
        payload_length: u32::from_be_bytes(bytes(header, 32)?),
        width: u16::from_be_bytes(bytes(header, 36)?),
        height: u16::from_be_bytes(bytes(header, 38)?),
    };
    if u64::from(metadata.payload_length)
        != u64::try_from(payload.len())
            .map_err(|_| Error::value("invalid v2 binary packet size"))?
    {
        return Err(Error::value("v2 binary payload length mismatch"));
    }
    if metadata.stream_handle == 0 || metadata.manifest_revision == 0 {
        return Err(Error::value("invalid v2 binary identity"));
    }
    match metadata.message_type {
        4 => {
            if !payload.is_empty()
                || metadata.width != 0
                || metadata.height != 0
                || !(1..=5).contains(&metadata.format_or_reason)
            {
                return Err(Error::value("invalid v2 CLEAR packet"));
            }
        }
        1 => {
            match metadata.format_or_reason {
                1 if !payload.starts_with(b"\x89PNG\r\n\x1a\n") => {
                    return Err(Error::value("invalid v2 PNG payload"))
                }
                2 if !payload.starts_with(b"\xff\xd8") || !payload.ends_with(b"\xff\xd9") => {
                    return Err(Error::value("invalid v2 JPEG payload"))
                }
                1 | 2 => {}
                _ => return Err(Error::value("invalid v2 image format")),
            }
            if metadata.width == 0 || metadata.height == 0 {
                return Err(Error::value("invalid v2 image dimensions"));
            }
        }
        2 | 3 => {
            if metadata.format_or_reason != 3
                || !(payload.starts_with(b"\x00\x00\x00\x01")
                    || payload.starts_with(b"\x00\x00\x01"))
            {
                return Err(Error::value("invalid v2 Annex-B payload"));
            }
            if metadata.width == 0 || metadata.height == 0 {
                return Err(Error::value("invalid v2 video dimensions"));
            }
        }
        _ => return Err(Error::value("invalid v2 binary message type")),
    }
    Ok((metadata, payload))
}
