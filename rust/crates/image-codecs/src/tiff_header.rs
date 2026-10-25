//! Metadata used by the original mode policy, before libtiff normalizes tags.
use super::modes;
use crate::DecodeError;
use std::collections::BTreeMap;
#[derive(Debug)]
pub(super) struct Header {
    pub mode: &'static str,
    pub rawmode: &'static str,
    pub extras: Vec<u16>,
    pub orientation: u16,
    pub compression: u16,
}
fn err(s: &str) -> DecodeError {
    DecodeError(s.into())
}
fn integer(b: &[u8], at: usize, n: usize, be: bool) -> Result<u64, DecodeError> {
    let data = b
        .get(
            at..at
                .checked_add(n)
                .ok_or_else(|| err("TIFF header overflow"))?,
        )
        .ok_or_else(|| err("TIFF header truncated"))?;
    let mut value = 0u64;
    if be {
        for v in data {
            value = (value << 8) | u64::from(*v);
        }
    } else {
        for v in data.iter().rev() {
            value = (value << 8) | u64::from(*v);
        }
    }
    Ok(value)
}
pub(super) fn parse(bytes: &[u8]) -> Result<Header, DecodeError> {
    let be = bytes.starts_with(b"MM");
    if !be && !bytes.starts_with(b"II") {
        return Err(err("TIFF byte order"));
    }
    let version = integer(bytes, 2, 2, be)?;
    let big = match version {
        42 => false,
        43 => true,
        _ => return Err(err("TIFF header version")),
    };
    let (word, entry, countsize, first) = if big {
        if integer(bytes, 4, 2, be)? != 8 || integer(bytes, 6, 2, be)? != 0 {
            return Err(err("BigTIFF offset size"));
        }
        (8, 20, 8, 8)
    } else {
        (4, 12, 2, 4)
    };
    let directory = usize::try_from(integer(bytes, first, word, be)?)
        .map_err(|_| err("TIFF directory overflow"))?;
    let count = usize::try_from(integer(bytes, directory, countsize, be)?)
        .map_err(|_| err("TIFF entry count overflow"))?;
    let start = directory
        .checked_add(countsize)
        .ok_or_else(|| err("TIFF directory overflow"))?;
    if count > bytes.len().saturating_sub(start) / entry {
        return Err(err("TIFF directory truncated"));
    }
    let mut tags = BTreeMap::<u16, Vec<u16>>::new();
    for i in 0..count {
        let at = start + i * entry;
        let tag = integer(bytes, at, 2, be)? as u16;
        if !matches!(tag, 258 | 259 | 262 | 266 | 274 | 277 | 284 | 338 | 339) {
            continue;
        }
        let kind = integer(bytes, at + 2, 2, be)?;
        let size = match kind {
            1 => 1,
            3 => 2,
            4 => 4,
            16 => 8,
            _ => return Err(err("TIFF metadata type")),
        };
        let n = usize::try_from(integer(bytes, at + 4, word, be)?)
            .map_err(|_| err("TIFF metadata count"))?;
        if n > 6 {
            return Err(err("TIFF metadata component count"));
        }
        let value = at + 4 + word;
        let data = if n * size <= word {
            value
        } else {
            usize::try_from(integer(bytes, value, word, be)?)
                .map_err(|_| err("TIFF metadata offset"))?
        };
        let mut values = Vec::new();
        for i in 0..n {
            values.push(
                u16::try_from(integer(
                    bytes,
                    data.checked_add(i * size)
                        .ok_or_else(|| err("TIFF value offset overflow"))?,
                    size,
                    be,
                )?)
                .map_err(|_| err("TIFF metadata value"))?,
            );
        }
        tags.insert(tag, values);
    }
    let scalar = |tag, default| {
        tags.get(&tag)
            .and_then(|v| v.first())
            .copied()
            .unwrap_or(default)
    };
    let photo = scalar(262, 1);
    let compression = scalar(259, 1);
    let fill = scalar(266, 1);
    let default_samples = if compression == 6 && matches!(photo, 2 | 6) {
        3
    } else {
        1
    };
    let mut channels = usize::from(scalar(277, default_samples));
    if channels > 6 || channels == 0 {
        return Err(err("TIFF sample count"));
    }
    let mut bits = tags.get(&258).cloned().unwrap_or_else(|| vec![1]);
    let mut extras = tags.get(&338).cloned().unwrap_or_default();
    let mut format = tags.get(&339).cloned().unwrap_or_else(|| vec![1]);
    if format.len() > 1 && format.iter().all(|v| *v == format[0]) {
        format.truncate(1);
    }
    if scalar(284, 1) == 2 && !extras.is_empty() && extras.iter().all(|v| *v == 0) {
        channels = channels
            .checked_sub(extras.len())
            .ok_or_else(|| err("TIFF extra sample count"))?;
        bits.truncate(bits.len().saturating_sub(extras.len()));
        extras.clear();
    }
    if channels < bits.len() {
        bits.truncate(channels);
    } else if channels > bits.len() && bits.len() == 1 {
        bits.resize(channels, bits[0]);
    }
    if bits.len() != channels || format.len() != 1 || bits.iter().any(|v| *v != bits[0]) {
        return Err(err("TIFF data organization"));
    }
    let (_, _, _, _, _, _, _, mode, rawmode) = modes::MODES
        .iter()
        .find(|(endian, p, f, order, depth, n, e, _, _)| {
            *endian == be
                && *p == photo
                && *f == format[0]
                && *order == fill
                && *depth == bits[0]
                && usize::from(*n) == channels
                && *e == extras
        })
        .ok_or_else(|| err("TIFF pixel mode not admitted by original"))?;
    Ok(Header {
        mode,
        rawmode,
        extras,
        orientation: scalar(274, 1),
        compression,
    })
}
