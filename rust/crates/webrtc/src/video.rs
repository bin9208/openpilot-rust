// Port of aiortc 1.14.0 codecs/h264.py encoded Packet path.
// Copyright (c) Jeremy Lainé. BSD-3-Clause; see AIORTC-LICENSE.
use crate::Error;
use num_traits::ToPrimitive;

#[cfg(feature = "native")]
mod debug;
#[cfg(feature = "native")]
mod debug_codec;
#[cfg(feature = "native")]
pub mod ipc;
#[cfg(feature = "native")]
pub(crate) mod track;

const PACKET_MAX: usize = 1300;
const DT: f64 = 0.05;
const CLOCK_RATE: f64 = 90_000.0;

#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error("empty NAL unit")]
    EmptyNalUnit,
    #[error("NAL length exceeds UInt16")]
    NalLength,
}

fn start_code(data: &[u8], offset: usize) -> Option<usize> {
    data.get(offset..)?
        .windows(3)
        .position(|window| window == [0, 0, 1])
        .map(|index| offset + index)
}

fn split_bitstream(data: &[u8]) -> Vec<&[u8]> {
    let mut units = Vec::new();
    let mut offset = 0;
    while let Some(start) = start_code(data, offset) {
        let nal_start = start + 3;
        if let Some(next) = start_code(data, nal_start) {
            let end = if data[next - 1] == 0 { next - 1 } else { next };
            units.push(&data[nal_start..end]);
            offset = next;
        } else {
            units.push(&data[nal_start..]);
            break;
        }
    }
    units
}

fn fragment(data: &[u8], payloads: &mut Vec<Vec<u8>>) {
    let payload_size = data.len() - 1;
    let num_packets = payload_size.div_ceil(PACKET_MAX - 2);
    let larger_packets = payload_size % num_packets;
    let package_size = payload_size / num_packets;
    let indicator = (data[0] & 0xe0) | 0x1c;
    let mut offset = 1;
    for index in 0..num_packets {
        let size = package_size + usize::from(index < larger_packets);
        let mut header = data[0] & 0x1f;
        if index == 0 {
            header |= 0x80;
        }
        if index + 1 == num_packets {
            header |= 0x40;
        }
        let mut packet = vec![indicator, header];
        packet.extend_from_slice(&data[offset..offset + size]);
        payloads.push(packet);
        offset += size;
    }
}

fn aggregate<'a>(
    data: &'a [u8],
    units: &mut impl Iterator<Item = &'a [u8]>,
) -> Result<(Vec<u8>, Option<&'a [u8]>), PackError> {
    let mut header = 0x18 | (data.first().ok_or(PackError::EmptyNalUnit)? & 0xe0);
    let mut available = Some(PACKET_MAX - 3);
    let mut counter = 0;
    let mut payload = Vec::new();
    let mut nal = Some(data);
    while let Some(unit) = nal {
        if available.is_none_or(|remaining| unit.len() > remaining) || counter == 9 {
            break;
        }
        let first = unit.first().ok_or(PackError::EmptyNalUnit)?;
        header |= first & 0x80;
        let nri = first & 0x60;
        if header & 0x60 < nri {
            header = (header & 0x9f) | nri;
        }
        available = available.and_then(|remaining| remaining.checked_sub(2 + unit.len()));
        counter += 1;
        payload.extend_from_slice(
            &u16::try_from(unit.len())
                .map_err(|_| PackError::NalLength)?
                .to_be_bytes(),
        );
        payload.extend_from_slice(unit);
        nal = units.next();
    }
    if counter == 0 {
        nal = units.next();
    }
    if counter <= 1 {
        Ok((data.to_vec(), nal))
    } else {
        payload.insert(0, header);
        Ok((payload, nal))
    }
}

/// Packs Annex B H264 bytes with the original aiortc Packet algorithm.
///
/// # Errors
/// Rejects empty NAL units and unrepresentable STAP-A lengths.
pub fn pack(data: &[u8]) -> Result<Vec<Vec<u8>>, PackError> {
    let mut result = Vec::new();
    let mut units = split_bitstream(data).into_iter();
    let mut current = units.next();
    while let Some(unit) = current {
        if unit.len() > PACKET_MAX {
            fragment(unit, &mut result);
            current = units.next();
        } else {
            let (payload, next) = aggregate(unit, &mut units)?;
            result.push(payload);
            current = next;
        }
    }
    Ok(result)
}

pub struct FrameClock {
    pts: f64,
    source_time: bool,
}

impl FrameClock {
    #[must_use]
    pub const fn new(use_source_frame_timestamps: bool) -> Self {
        Self {
            pts: 0.0,
            source_time: use_source_frame_timestamps,
        }
    }

    /// Advances PTS using the source floating-point operation order.
    ///
    /// # Errors
    /// Returns an error if PTS is outside the packet's signed integer range.
    pub fn next(&mut self, frame: u32) -> Result<i64, Error> {
        let packet_pts = if self.source_time {
            (f64::from(frame) * DT * CLOCK_RATE).trunc()
        } else {
            self.pts
        };
        self.pts = packet_pts + DT * CLOCK_RATE;
        packet_pts
            .to_i64()
            .ok_or(Error::Contract("video PTS range"))
    }
}
