//! BLP's CC0 decoder math differs from the native BCn decoder used by FTEX.
use super::{array, check_dimensions, le32, range, Array, Result};
fn raw(mode: &str, w: usize, h: usize, data: &[u8]) -> Result<Array> {
    let ch = mode.len();
    Ok(array(
        mode,
        w,
        h,
        ch,
        range(data, 0, w * h * ch)?
            .iter()
            .copied()
            .map(i64::from)
            .collect(),
    ))
}
pub(super) fn ftex(b: &[u8]) -> Result<Array> {
    let (w, h) = (le32(b, 8)?, le32(b, 12)?);
    check_dimensions(w, h)?;
    if le32(b, 20)? != 1 {
        return Err("FTEX format count".into());
    }
    let at = le32(b, 28)?;
    let data = range(b, at + 4, le32(b, at)?)?;
    match le32(b, 24)? {
        1 => raw("RGB", w, h, data),
        0 => {
            // The existing image crate's native DXT1 decoder uses the same BCn
            // integer math. A DDS header only transports dimensions and FourCC.
            let mut dds = vec![0; 128];
            dds[..4].copy_from_slice(b"DDS ");
            for (at, v) in [
                (4, 124u32),
                (8, 0x81007),
                (12, u32::try_from(h)?),
                (16, u32::try_from(w)?),
                (20, u32::try_from(data.len())?),
                (76, 32),
                (80, 4),
                (108, 0x1000),
            ] {
                dds[at..at + 4].copy_from_slice(&v.to_le_bytes());
            }
            dds[84..88].copy_from_slice(b"DXT1");
            dds.extend_from_slice(data);
            let image =
                image::load_from_memory_with_format(&dds, image::ImageFormat::Dds)?.into_rgba8();
            raw("RGBA", w, h, image.as_raw())
        }
        _ => Err("unsupported FTEX compression".into()),
    }
}
pub(super) fn blp(b: &[u8]) -> Result<Array> {
    let first = b.starts_with(b"BLP1");
    let compression = le32(b, 4)?;
    let (w, h) = (le32(b, 12)?, le32(b, 16)?);
    check_dimensions(w, h)?;
    let alpha = if first {
        le32(b, 8)? != 0
    } else {
        range(b, 9, 1)?[0] != 0
    };
    let mode = if alpha { "RGBA" } else { "RGB" };
    let ch = mode.len();
    let encoding = if first {
        le32(b, 20)?
    } else {
        range(b, 8, 1)?[0] as usize
    };
    let table = if first { 28 } else { 20 };
    let offset = le32(b, table)?;
    let length = le32(b, table + 64)?;
    let at = table + 128;
    if first && compression == 0 {
        let headlen = le32(b, at)?;
        let mut jpeg = range(b, at + 4, headlen)?.to_vec();
        jpeg.extend_from_slice(range(b, offset, length)?);
        let decoded = super::super::decode(&jpeg)?;
        let mut pixels = Vec::new();
        pixels.try_reserve_exact(w * h * 3)?;
        match decoded.mode.as_str() {
            "L" => {
                for v in decoded.values {
                    pixels.extend([v as u8; 3]);
                }
            }
            "RGB" => {
                for p in decoded.values.chunks_exact(3) {
                    pixels.extend([p[2] as u8, p[1] as u8, p[0] as u8]);
                }
            }
            "CMYK" => {
                for p in decoded.values.chunks_exact(4) {
                    // The JPEG rawmode remains CMYK;I; BLP then converts the loaded
                    // CMYK values to RGB and unpacks them in BGR order.
                    let k = 255 - p[3];
                    for c in [2, 1, 0] {
                        pixels.push((k - (p[c] * k + 127) / 255) as u8);
                    }
                }
            }
            _ => return Err("unsupported BLP JPEG mode".into()),
        }
        return raw("RGB", w, h, &pixels);
    }
    if compression != 1 {
        return Err("unsupported BLP compression".into());
    }
    let palette = range(b, at, 1024)?;
    let data = if first {
        range(b, at + 1024, length)?
    } else {
        range(b, offset, length)?
    };
    if (first && matches!(encoding, 4 | 5)) || (!first && encoding == 1) {
        let mut pixels = Vec::new();
        pixels.try_reserve_exact(data.len() * ch)?;
        for idx in data {
            let p = range(palette, *idx as usize * 4, 4)?;
            pixels.extend([p[2], p[1], p[0]]);
            if alpha {
                pixels.push(p[3]);
            }
        }
        return raw(mode, w, h, &pixels);
    }
    if first || encoding != 2 {
        return Err("unsupported BLP encoding".into());
    }
    let kind = range(b, 10, 1)?[0];
    if ![0, 1, 7].contains(&kind) {
        return Err("unsupported BLP alpha encoding".into());
    }
    let size = if kind == 0 { 8 } else { 16 };
    let blocks = w.div_ceil(4);
    let outch = if kind != 0 || alpha { 4 } else { 3 };
    let mut pixels = Vec::new();
    pixels.try_reserve_exact(blocks * 4 * h.div_ceil(4) * 4 * outch)?;
    for by in 0..h.div_ceil(4) {
        let mut rows = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
        for bx in 0..blocks {
            let block = range(data, (by * blocks + bx) * size, size)?;
            let colors = &block[size - 8..];
            let c0 = u16::from_le_bytes(colors[..2].try_into()?);
            let c1 = u16::from_le_bytes(colors[2..4].try_into()?);
            let unpack = |v: u16| [(v >> 11 & 31) * 8, (v >> 5 & 63) * 4, (v & 31) * 8];
            let (a, b) = (unpack(c0), unpack(c1));
            let mut table = [[0u16; 3]; 4];
            table[0] = a;
            table[1] = b;
            for c in 0..3 {
                if kind != 0 || c0 > c1 {
                    table[2][c] = (2 * a[c] + b[c]) / 3;
                    table[3][c] = (a[c] + 2 * b[c]) / 3;
                } else {
                    table[2][c] = (a[c] + b[c]) / 2;
                }
            }
            let bits = u32::from_le_bytes(colors[4..8].try_into()?);
            let mut alphatable = [0u16; 8];
            let mut alphabits = 0u64;
            if kind == 7 {
                alphatable[0] = u16::from(block[0]);
                alphatable[1] = u16::from(block[1]);
                for i in 2..8 {
                    alphatable[i] = if block[0] > block[1] {
                        ((8 - i) as u16 * alphatable[0] + (i - 1) as u16 * alphatable[1]) / 7
                    } else if i < 6 {
                        ((6 - i) as u16 * alphatable[0] + (i - 1) as u16 * alphatable[1]) / 5
                    } else if i == 6 {
                        0
                    } else {
                        255
                    };
                }
                for (i, v) in block[2..8].iter().enumerate() {
                    alphabits |= u64::from(*v) << (i * 8);
                }
            }
            for (y, row) in rows.iter_mut().enumerate() {
                for x in 0..4 {
                    let i = y * 4 + x;
                    let control = ((bits >> (i * 2)) & 3) as usize;
                    row.extend(table[control].map(|v| v as u8));
                    if outch == 4 {
                        row.push(match kind {
                            0 => {
                                if c0 <= c1 && control == 3 {
                                    0
                                } else {
                                    255
                                }
                            }
                            1 => ((block[i / 2] >> ((i % 2) * 4)) & 15) * 17,
                            _ => alphatable[((alphabits >> (i * 3)) & 7) as usize] as u8,
                        });
                    }
                }
            }
        }
        for row in rows {
            pixels.extend(row);
        }
    }
    // Pillow's Python BLP decoder concatenates complete padded block rows and
    // passes the resulting stream to raw mode; it does not crop each block row.
    raw(mode, w, h, &pixels)
}
