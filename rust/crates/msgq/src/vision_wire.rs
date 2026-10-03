use crate::{
    vision_types::{VisionLayout, VisionMetadata, VisionStream},
    Error,
};

pub(crate) const BUFFER_BYTES: usize = 112;
pub(crate) const PACKET_BYTES: usize = 48;
pub(crate) const MAX_FDS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Buffer {
    pub layout: VisionLayout,
    pub mapped_length: usize,
    pub server_id: u64,
    pub index: usize,
    pub stream: VisionStream,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Packet {
    pub server_id: u64,
    pub index: usize,
    pub frame_id: u32,
    pub timestamp_sof: u64,
    pub timestamp_eof: u64,
    pub valid: bool,
}

fn bytes<const N: usize>(source: &[u8], offset: usize) -> Result<[u8; N], Error> {
    source
        .get(offset..offset + N)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(Error::Corrupt("truncated VisionIPC field"))
}

fn size(source: &[u8], offset: usize) -> Result<usize, Error> {
    usize::try_from(u64::from_le_bytes(bytes(source, offset)?))
        .map_err(|_| Error::Corrupt("VisionIPC size exceeds the address space"))
}

fn write_size(destination: &mut [u8], offset: usize, value: usize) -> Result<(), Error> {
    let value = u64::try_from(value).map_err(|_| Error::Invalid("VisionIPC size overflow"))?;
    destination[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    Ok(())
}

pub(crate) fn decode_buffers(
    payload: &[u8],
    descriptor_count: usize,
    requested: VisionStream,
) -> Result<Vec<Buffer>, Error> {
    if descriptor_count == 0
        || descriptor_count > MAX_FDS
        || payload.len() != descriptor_count * BUFFER_BYTES
    {
        return Err(Error::Corrupt("VisionIPC buffer/descriptor count mismatch"));
    }
    let mut result = Vec::with_capacity(descriptor_count);
    let mut server_id = None;
    for (index, record) in payload.chunks_exact(BUFFER_BYTES).enumerate() {
        let buffer = Buffer {
            layout: VisionLayout {
                width: size(record, 40)?,
                height: size(record, 48)?,
                stride: size(record, 56)?,
                uv_offset: size(record, 64)?,
                len: size(record, 0)?,
            },
            mapped_length: size(record, 8)?,
            server_id: u64::from_le_bytes(bytes(record, 88)?),
            index: size(record, 96)?,
            stream: VisionStream::from_native(i32::from_le_bytes(bytes(record, 104)?))?,
        };
        if buffer.index != index || buffer.stream != requested {
            return Err(Error::Corrupt("VisionIPC buffer index or stream mismatch"));
        }
        if server_id.is_some_and(|expected| expected != buffer.server_id) {
            return Err(Error::Corrupt(
                "VisionIPC buffers belong to different servers",
            ));
        }
        server_id = Some(buffer.server_id);
        buffer.layout.validate(buffer.mapped_length)?;
        result.push(buffer);
    }
    Ok(result)
}

pub(crate) fn encode_buffer(
    buffer: Buffer,
    address: usize,
    fd: i32,
) -> Result<[u8; BUFFER_BYTES], Error> {
    let mut result = [0; BUFFER_BYTES];
    let frame_id = address
        .checked_add(buffer.layout.len)
        .ok_or(Error::Invalid("VisionIPC frame-ID address overflow"))?;
    let uv = address
        .checked_add(buffer.layout.uv_offset)
        .ok_or(Error::Invalid("VisionIPC UV address overflow"))?;
    for (offset, value) in [
        (0, buffer.layout.len),
        (8, buffer.mapped_length),
        (16, address),
        (24, frame_id),
        (40, buffer.layout.width),
        (48, buffer.layout.height),
        (56, buffer.layout.stride),
        (64, buffer.layout.uv_offset),
        (72, address),
        (80, uv),
        (96, buffer.index),
    ] {
        write_size(&mut result, offset, value)?;
    }
    result[32..36].copy_from_slice(&fd.to_le_bytes());
    result[88..96].copy_from_slice(&buffer.server_id.to_le_bytes());
    result[104..108].copy_from_slice(&buffer.stream.native().to_le_bytes());
    Ok(result)
}

pub(crate) fn decode_packet(payload: &[u8]) -> Result<Packet, Error> {
    if payload.len() != PACKET_BYTES {
        return Err(Error::Corrupt("invalid VisionIPC frame packet size"));
    }
    let valid = match payload[40] {
        0 => false,
        1 => true,
        _ => return Err(Error::Corrupt("invalid VisionIPC validity bit")),
    };
    Ok(Packet {
        server_id: u64::from_le_bytes(bytes(payload, 0)?),
        index: size(payload, 8)?,
        frame_id: u32::from_le_bytes(bytes(payload, 16)?),
        timestamp_sof: u64::from_le_bytes(bytes(payload, 24)?),
        timestamp_eof: u64::from_le_bytes(bytes(payload, 32)?),
        valid,
    })
}

pub(crate) fn encode_packet(
    server_id: u64,
    index: usize,
    metadata: VisionMetadata,
) -> Result<[u8; PACKET_BYTES], Error> {
    let mut result = [0; PACKET_BYTES];
    result[0..8].copy_from_slice(&server_id.to_le_bytes());
    write_size(&mut result, 8, index)?;
    result[16..20].copy_from_slice(&metadata.frame_id.to_le_bytes());
    result[24..32].copy_from_slice(&metadata.timestamp_sof.to_le_bytes());
    result[32..40].copy_from_slice(&metadata.timestamp_eof.to_le_bytes());
    result[40] = u8::from(metadata.valid);
    Ok(result)
}

pub(crate) fn decode_streams(payload: &[u8]) -> Result<Vec<VisionStream>, Error> {
    if payload.len() > 16 || !payload.len().is_multiple_of(4) {
        return Err(Error::Corrupt("invalid VisionIPC discovery response"));
    }
    let mut present = [false; 4];
    for offset in (0..payload.len()).step_by(4) {
        let stream = VisionStream::from_native(i32::from_le_bytes(bytes(payload, offset)?))?;
        let index = usize::try_from(stream.native())
            .map_err(|_| Error::Corrupt("invalid VisionIPC stream"))?;
        present[index] = true;
    }
    Ok([
        VisionStream::Road,
        VisionStream::Driver,
        VisionStream::WideRoad,
        VisionStream::Map,
    ]
    .into_iter()
    .zip(present)
    .filter_map(|(stream, present)| present.then_some(stream))
    .collect())
}

#[cfg(test)]
#[path = "vision_wire_tests.rs"]
mod tests;
