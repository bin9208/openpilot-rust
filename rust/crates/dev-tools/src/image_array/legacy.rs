use super::{array, check_dimensions, numpy_float, zeroed, Array, Result};
fn range(b: &[u8], start: usize, len: usize) -> Result<&[u8]> {
    Ok(
        b.get(start..start.checked_add(len).ok_or("offset overflow")?)
            .ok_or("truncated image")?,
    )
}
fn be16(b: &[u8], p: usize) -> Result<usize> {
    Ok(u16::from_be_bytes(range(b, p, 2)?.try_into()?) as usize)
}
fn be32(b: &[u8], p: usize) -> Result<usize> {
    Ok(u32::from_be_bytes(range(b, p, 4)?.try_into()?) as usize)
}
fn le32(b: &[u8], p: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(range(b, p, 4)?.try_into()?))
}
fn le16(b: &[u8], p: usize) -> Result<usize> {
    Ok(u16::from_le_bytes(range(b, p, 2)?.try_into()?) as usize)
}
pub(super) fn bmp(b: &[u8]) -> Result<Option<Array>> {
    let header = le32(b, 14)? as usize;
    if header < 40 {
        return Ok(None);
    }
    let bits = le16(b, 28)?;
    if bits > 8 {
        return Ok(None);
    }
    if ![1, 4, 8].contains(&bits) {
        return Err("unsupported BMP indexed bit depth".into());
    }
    let compression = le32(b, 30)?;
    if compression != 0 && compression != 1 && compression != 2 {
        return Err("unsupported BMP compression".into());
    }
    let w = le32(b, 18)? as usize;
    let signedh = le32(b, 22)? as i32;
    let h = signedh.unsigned_abs() as usize;
    check_dimensions(w, h)?;
    let count = le32(b, 46)? as usize;
    let count = if count == 0 { 1 << bits } else { count };
    let palette = range(b, 14 + header, count * 4)?;
    let identity = palette
        .chunks_exact(4)
        .enumerate()
        .all(|(i, v)| v[..3] == [i as u8; 3]);
    let mono =
        bits == 1 && palette.len() >= 8 && palette[..3] == [0; 3] && palette[4..7] == [255; 3];
    let mode = if mono {
        "1"
    } else if identity {
        "L"
    } else {
        "P"
    };
    if compression != 0 {
        let mut at = le32(b, 10)? as usize;
        let mut data: Vec<u8> = Vec::new();
        data.try_reserve_exact(w * h)?;
        let mut x = 0usize;
        // The original decoder clips encoded runs at a row, pads EOL/delta
        // with zeroes, and rejects a short decoded raster at set_as_raw.
        while data.len() < w * h {
            let Some(pair) = b.get(at..at.saturating_add(2)) else {
                break;
            };
            at += 2;
            let count = pair[0] as usize;
            let command = pair[1];
            if count > 0 {
                let count = count.min(w.saturating_sub(x));
                for i in 0..count {
                    data.push(if compression == 2 {
                        if i % 2 == 0 {
                            command >> 4
                        } else {
                            command & 15
                        }
                    } else {
                        command
                    });
                }
                x += count;
            } else {
                match command {
                    0 => {
                        let padding = (w - data.len() % w) % w;
                        data.try_reserve(padding)?;
                        data.resize(data.len() + padding, 0);
                        x = 0;
                    }
                    1 => break,
                    2 => {
                        let Some(delta) = b.get(at..at.saturating_add(2)) else {
                            break;
                        };
                        at += 2;
                        let count = usize::from(delta[0]) + usize::from(delta[1]) * w;
                        if data.len().checked_add(count).is_none_or(|n| n > w * h) {
                            return Err("BMP RLE delta exceeds raster".into());
                        }
                        data.resize(data.len() + count, 0);
                        x = data.len() % w;
                    }
                    count => {
                        let bytes = if compression == 2 {
                            usize::from(count) / 2
                        } else {
                            usize::from(count)
                        };
                        let stop = at.saturating_add(bytes).min(b.len());
                        let input = &b[at.min(b.len())..stop];
                        for &value in input {
                            if compression == 2 {
                                data.push(value >> 4);
                                data.push(value & 15);
                            } else {
                                data.push(value)
                            }
                        }
                        if input.len() < bytes {
                            break;
                        }
                        at += bytes;
                        x += usize::from(count);
                        if !at.is_multiple_of(2) {
                            at += 1;
                        }
                    }
                }
            }
        }
        if data.len() < w * h {
            return Err("not enough BMP RLE image data".into());
        }
        let mut values = zeroed(w * h)?;
        for y in 0..h {
            for x in 0..w {
                values[(if signedh < 0 { y } else { h - 1 - y }) * w + x] =
                    i64::from(data[y * w + x]);
            }
        }
        return Ok(Some(array(mode, w, h, 1, values)));
    }
    let rowsize = (w * bits).div_ceil(32) * 4;
    let start = le32(b, 10)? as usize;
    let mut vals = vec![];
    vals.try_reserve_exact(w * h * if mode == "RGB" { 3 } else { 1 })?;
    for y in 0..h {
        let row = range(
            b,
            start + if signedh < 0 { y } else { h - 1 - y } * rowsize,
            rowsize,
        )?;
        for x in 0..w {
            vals.push(i64::from(
                (row[x * bits / 8] >> (8 - bits - (x * bits % 8))) & ((1u16 << bits) - 1) as u8,
            ));
        }
    }
    Ok(Some(array(mode, w, h, 1, vals)))
}
pub(super) fn tga(b: &[u8]) -> Result<Option<Array>> {
    range(b, 0, 18)?;
    let kind = b[2];
    if ![1, 3, 9, 11].contains(&kind) {
        return Ok(None);
    }
    let w = le16(b, 12)?;
    let h = le16(b, 14)?;
    check_dimensions(w, h)?;
    let bits = b[16] as usize;
    if ![1, 8].contains(&bits) {
        return Ok(None);
    }
    let offset = 18 + b[0] as usize + le16(b, 5)? * (b[7] as usize).div_ceil(8);
    let total = w * h;
    let mut samples = vec![];
    samples.try_reserve_exact(total)?;
    if kind >= 9 {
        let mut at = offset;
        while samples.len() < total {
            let tag = *b.get(at).ok_or("TGA RLE truncated")?;
            at += 1;
            let count = usize::from(tag & 127) + 1;
            if samples.len() + count > total {
                return Err("TGA RLE overrun".into());
            }
            if tag & 128 != 0 {
                samples.extend(std::iter::repeat_n(
                    *b.get(at).ok_or("TGA RLE sample")?,
                    count,
                ));
                at += 1;
            } else {
                samples.extend_from_slice(range(b, at, count)?);
                at += count;
            }
        }
    } else if bits == 1 {
        for y in 0..h {
            let row = range(b, offset + y * w.div_ceil(8), w.div_ceil(8))?;
            for x in 0..w {
                samples.push((row[x / 8] >> (7 - x % 8)) & 1)
            }
        }
    } else {
        samples.extend_from_slice(range(b, offset, total)?)
    }
    let mode = if kind == 1 || kind == 9 {
        "P"
    } else if bits == 1 {
        "1"
    } else {
        "L"
    };
    let mut vals = zeroed(total)?;
    for y in 0..h {
        for x in 0..w {
            let tx = if b[17] & 16 != 0 { w - 1 - x } else { x };
            let ty = if b[17] & 32 != 0 { y } else { h - 1 - y };
            vals[ty * w + tx] = i64::from(samples[y * w + x]);
        }
    }
    Ok(Some(array(mode, w, h, 1, vals)))
}
pub(super) fn pcx(b: &[u8]) -> Result<Array> {
    range(b, 0, 128)?;
    let bits = b[3] as usize;
    let w = le16(b, 8)?.checked_sub(le16(b, 4)?).ok_or("PCX width")? + 1;
    let h = le16(b, 10)?.checked_sub(le16(b, 6)?).ok_or("PCX height")? + 1;
    check_dimensions(w, h)?;
    let planes = b[65] as usize;
    let stride = le16(b, 66)?;
    if ![1, 8].contains(&bits) || planes == 0 || planes > 4 || stride < (w * bits).div_ceil(8) {
        return Err("PCX invalid planes or scanline stride".into());
    }
    let need = h * planes * stride;
    let mut data = vec![];
    data.try_reserve_exact(need)?;
    let mut at = 128;
    while data.len() < need {
        let v = *b.get(at).ok_or("PCX truncated")?;
        at += 1;
        let (count, v) = if b[2] == 1 && v & 192 == 192 {
            let sample = *b.get(at).ok_or("PCX RLE truncated")?;
            at += 1;
            ((v & 63) as usize, sample)
        } else {
            (1, v)
        };
        if count == 0 || data.len() + count > need {
            return Err("PCX RLE overrun".into());
        }
        data.extend(std::iter::repeat_n(v, count));
    }
    let (mode, ch) = if bits == 8 && planes == 3 {
        ("RGB", 3)
    } else if bits == 1 && planes == 1 {
        ("1", 1)
    } else if bits == 8 && planes == 1 {
        let p = b.get(b.len().saturating_sub(768)..).ok_or("PCX palette")?;
        if p.chunks_exact(3)
            .enumerate()
            .all(|(i, v)| v == [i as u8; 3])
        {
            ("L", 1)
        } else {
            ("P", 1)
        }
    } else if bits == 1 && planes <= 4 {
        ("P", 1)
    } else {
        return Err("PCX unsupported layout".into());
    };
    let mut vals = vec![];
    vals.try_reserve_exact(w * h * ch)?;
    for y in 0..h {
        for x in 0..w {
            if ch == 3 {
                for c in 0..3 {
                    vals.push(i64::from(data[(y * planes + c) * stride + x]));
                }
            } else if bits == 8 {
                vals.push(i64::from(data[y * stride + x]));
            } else {
                let mut v = 0;
                for p in 0..planes {
                    v |= ((data[(y * planes + p) * stride + x / 8] >> (7 - x % 8)) & 1) << p;
                }
                vals.push(i64::from(v));
            }
        }
    }
    Ok(array(mode, w, h, ch, vals))
}
pub(super) fn pbm(b: &[u8]) -> Result<Array> {
    let mut at = 2;
    let mut tokens = vec![];
    while tokens.len() < 2 {
        while b.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1
        }
        if b.get(at) == Some(&b'#') {
            while b.get(at).is_some_and(|v| *v != b'\n') {
                at += 1
            }
            continue;
        }
        let start = at;
        while b.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1
        }
        if start == at {
            return Err("PBM dimensions".into());
        }
        tokens.push(std::str::from_utf8(&b[start..at])?.parse::<usize>()?);
    }
    if !b.get(at).is_some_and(u8::is_ascii_whitespace) {
        return Err("PBM header terminator".into());
    }
    at += 1;
    let (w, h) = (tokens[0], tokens[1]);
    check_dimensions(w, h)?;
    let mut vals = vec![];
    vals.try_reserve_exact(w * h)?;
    for y in 0..h {
        let row = range(b, at + y * w.div_ceil(8), w.div_ceil(8))?;
        for x in 0..w {
            vals.push(i64::from(1 - ((row[x / 8] >> (7 - x % 8)) & 1)));
        }
    }
    Ok(array("1", w, h, 1, vals))
}
pub(super) fn sgi(b: &[u8]) -> Result<Array> {
    range(b, 0, 512)?;
    let compression = b[2];
    let precision = b[3] as usize;
    let dim = be16(b, 4)?;
    let w = be16(b, 6)?;
    let h = be16(b, 8)?;
    let ch = be16(b, 10)?;
    check_dimensions(w, h)?;
    let mode = match (precision, dim, ch) {
        (1 | 2, 1 | 2, 1) => "L",
        (1 | 2, 3, 3) => "RGB",
        (1 | 2, 3, 4) => "RGBA",
        _ => return Err("unsupported SGI mode".into()),
    };
    let size = w
        .checked_mul(h)
        .and_then(|n| n.checked_mul(ch))
        .ok_or("SGI size overflow")?;
    let mut vals = zeroed(size)?;
    for channel in 0..ch {
        for row in 0..h {
            let mut samples = vec![];
            samples.try_reserve_exact(w)?;
            if compression == 0 {
                let start = 512 + (channel * w * h + row * w) * precision;
                let data = range(b, start, w * precision)?;
                for v in data.chunks_exact(precision) {
                    samples.push(v[0] as i64)
                }
            } else if compression == 1 {
                let index = channel * h + row;
                let offset = be32(b, 512 + index * 4)?;
                let length = be32(b, 512 + h * ch * 4 + index * 4)?;
                let data = range(b, offset, length)?;
                let mut at = 0;
                loop {
                    let control = if precision == 2 {
                        be16(data, at)?
                    } else {
                        *data.get(at).ok_or("SGI RLE truncated")? as usize
                    };
                    at += precision;
                    let count = control & 127;
                    if count == 0 {
                        break;
                    }
                    if samples.len() + count > w {
                        return Err("SGI RLE overrun".into());
                    }
                    if control & 128 != 0 {
                        for _ in 0..count {
                            let value = *data.get(at).ok_or("SGI RLE literal truncated")?;
                            samples.push(value as i64);
                            at += precision;
                        }
                    } else {
                        let value = *data.get(at).ok_or("SGI RLE repeat truncated")?;
                        at += precision;
                        samples.extend(std::iter::repeat_n(value as i64, count));
                    }
                }
                if samples.len() != w {
                    return Err("SGI RLE short scanline".into());
                }
            } else {
                return Err("unknown SGI compression".into());
            }
            for (x, v) in samples.into_iter().enumerate() {
                vals[((h - 1 - row) * w + x) * ch + channel] = v;
            }
        }
    }
    Ok(array(mode, w, h, ch, vals))
}
pub(super) fn dds(b: &[u8]) -> Result<Option<Array>> {
    range(b, 0, 128)?;
    if le32(b, 4)? != 124 {
        return Err("DDS header size".into());
    }
    let flags = le32(b, 80)?;
    if flags & 4 != 0 {
        return Ok(None);
    }
    let h = le32(b, 12)? as usize;
    let w = le32(b, 16)? as usize;
    check_dimensions(w, h)?;
    let bits = le32(b, 88)? as usize;
    if ![8, 16, 24, 32].contains(&bits) {
        return Err("DDS unsupported bit size".into());
    }
    let masks = [le32(b, 92)?, le32(b, 96)?, le32(b, 100)?, le32(b, 104)?];
    let alpha = flags & 1 != 0;
    let lum = flags & 0x20000 != 0;
    let mode = if lum {
        if alpha {
            "LA"
        } else {
            "L"
        }
    } else if alpha {
        "RGBA"
    } else {
        "RGB"
    };
    let ch = mode.len();
    let row = w.checked_mul(bits / 8).ok_or("DDS width overflow")?;
    let pitch = if le32(b, 8)? & 8 != 0 {
        (le32(b, 20)? as usize).max(row)
    } else {
        row
    };
    let mut values = vec![];
    values.try_reserve_exact(w * h * ch)?;
    for y in 0..h {
        let data = range(b, 128 + y * pitch, row)?;
        for sample in data.chunks_exact(bits / 8) {
            let mut word = 0u32;
            for (i, v) in sample.iter().enumerate() {
                word |= u32::from(*v) << (i * 8)
            }
            let component = |mask: u32| -> i64 {
                if mask == 0 {
                    return 0;
                }
                let shift = mask.trailing_zeros();
                let max = mask >> shift;
                i64::from((u64::from((word & mask) >> shift) * 255 / u64::from(max)) as u32)
            };
            if lum {
                values.push(i64::from(sample[0]));
                if alpha {
                    values.push(i64::from(*sample.get(1).ok_or("DDS LA width")?));
                }
            } else {
                for mask in &masks[..ch] {
                    values.push(component(*mask));
                }
            }
        }
    }
    Ok(Some(array(mode, w, h, ch, values)))
}
pub(super) fn im(b: &[u8]) -> Result<Option<Array>> {
    if !b.get(..100).unwrap_or(b).contains(&b'\n') {
        return Ok(None);
    }
    let Some(end) = b.iter().position(|v| *v == 26) else {
        return Ok(None);
    };
    let header = String::from_utf8_lossy(&b[..end]);
    let mut typ = "Greyscale image";
    let (mut w, mut h) = (512usize, 512usize);
    let mut lut = false;
    let mut recognized = false;
    for line in header.split('\n') {
        let line = line.trim_matches(['\r', '\0']);
        if line.len() > 100 {
            return Ok(None);
        }
        let Some((key, value)) = line.split_once(':') else {
            if line.is_empty() {
                continue;
            } else {
                return Ok(None);
            }
        };
        let value = value.trim();
        match key {
            "Image type" => {
                typ = value;
                recognized = true
            }
            "Image size (x*y)" => {
                let Some((x, y)) = value.split_once('*').or_else(|| value.split_once(',')) else {
                    return Err("IM size syntax".into());
                };
                w = x.trim().parse()?;
                h = y.trim().parse()?;
                recognized = true
            }
            "Lut" => {
                lut = true;
                recognized = true
            }
            "File size (no of images)"
            | "Comment"
            | "Date"
            | "Digitalization equipment"
            | "Name"
            | "Scale (x,y)" => recognized = true,
            _ => {}
        }
    }
    if !recognized {
        return Ok(None);
    }
    // Parse exactly the rawmodes admitted by Pillow's IM plugin. RLB/RYB and
    // PA;L advertise a mode but fail its raw decoder; preserve those errors.
    if ["RLB image", "RYB image"].contains(&typ) {
        return Err("unknown raw mode for given IM image mode".into());
    }
    let float_type = match typ {
        "L 8 image" | "L*8 image" => Some((8, false, false, false)),
        "L 8S image" | "L*8S image" => Some((8, true, false, false)),
        "L*16 image" => Some((16, false, false, true)),
        "L 16S image" | "L*16S image" => Some((16, true, false, false)),
        "L 32 image" | "L*32 image" | "L 32 F image" => Some((32, false, false, false)),
        "L 32F image" | "L*32F image" => Some((32, false, true, false)),
        other => other
            .strip_prefix("L*")
            .and_then(|v| v.strip_suffix(" image"))
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|bits| (2..32).contains(bits))
            .map(|bits| (bits, false, false, true)),
    };
    let (mut mode, ch, precision, planar) = if let Some((bits, _, _, _)) = float_type {
        ("F", 1, bits.div_ceil(8), false)
    } else {
        match typ {
            "0 1 image" | "L 1 image" | "B1 image" => ("1", 1, 0, false),
            "B2 image" | "B4 image" => ("P", 1, 0, false),
            "Greyscale image" | "Grayscale image" => ("L", 1, 1, false),
            "RGB image" | "RGB3 image" | "RYB3 image" => ("RGB", 3, 1, true),
            "RGBA image" => ("RGBA", 4, 1, true),
            "RGBX image" => ("RGB", 4, 1, true),
            "LA image" | "PA image" => ("LA", 2, 1, true),
            "CMYK image" => ("CMYK", 4, 1, true),
            "YCC image" => ("YCbCr", 3, 1, true),
            "X 24 image" => ("RGB", 3, 1, false),
            "L 32 S image" | "L 32S image" | "L*32S image" => ("I", 1, 4, false),
            "L 16 image" => ("I;16", 1, 2, false),
            "L 16L image" | "L*16L image" => ("I;16L", 1, 2, false),
            "L 16B image" | "L*16B image" => ("I;16B", 1, 2, false),
            _ => return Err(format!("unknown IM rawmode {typ}").into()),
        }
    };
    check_dimensions(w, h)?;
    let mut offset = end + 1;
    if lut {
        let data = range(b, offset, 768)?;
        offset += 768;
        let gray = (0..256).all(|i| data[i] == data[256 + i] && data[i] == data[512 + i]);
        if !gray {
            if mode == "L" || mode == "P" {
                mode = "P";
            } else if mode == "LA" {
                mode = "PA";
            }
        }
    }
    if typ == "PA image" && mode == "LA" {
        return Err("unknown raw mode for given IM image mode".into());
    }
    let outch = if typ == "RGBX image" { 3 } else { ch };
    let bits = match typ {
        "B2 image" => 2,
        "B4 image" => 4,
        _ => 1,
    };
    let rowsize = if precision == 0 {
        (w * bits).div_ceil(8)
    } else {
        w.checked_mul(ch)
            .and_then(|n| n.checked_mul(precision))
            .ok_or("IM size overflow")?
    };
    let mut values = zeroed(
        w.checked_mul(h)
            .and_then(|n| n.checked_mul(outch))
            .ok_or("IM allocation overflow")?,
    )?;
    if let Some((bits, _, _, true)) = float_type {
        let (mut buffer, mut buffered, mut at) = (0u64, 0usize, offset);
        let mask = (1u64 << bits) - 1;
        for y in 0..h {
            for x in 0..w {
                while buffered < bits {
                    buffer |= u64::from(*b.get(at).ok_or("IM packed float truncated")?) << buffered;
                    buffered += 8;
                    at += 1;
                }
                let sample = buffer & mask;
                buffer >>= bits;
                buffered -= bits;
                values[(h - 1 - y) * w + x] = numpy_float(f64::from(sample as f32));
            }
            // Preserve the bit decoder's row reset: the count resets while the
            // unused bit-buffer prefix remains for the next row's OR fill.
            buffered = 0;
        }
        return Ok(Some(array(mode, w, h, 1, values)));
    }
    for y in 0..h {
        for x in 0..w {
            for c in 0..outch {
                let tile = typ == "RGB3 image" || typ == "RYB3 image";
                let p = if tile {
                    let band = if c == 0 {
                        1
                    } else if c == 1 {
                        0
                    } else {
                        2
                    };
                    offset + (band * h + y) * w + x
                } else if planar {
                    offset + y * rowsize + (c * w + x) * precision
                } else {
                    offset + y * rowsize + (x * ch + c) * precision
                };
                let v = if let Some((bits, signed, ieee, packed)) = float_type {
                    let value = if packed {
                        // Packed float integer fields are little-bit first and each
                        // row consumes a whole number of input bytes.
                        let start = offset + y * (w * bits).div_ceil(8);
                        let bit = x * bits;
                        let mut raw = 0u32;
                        for n in 0..bits {
                            raw |= u32::from(
                                (*b.get(start + (bit + n) / 8)
                                    .ok_or("IM packed float truncated")?
                                    >> ((bit + n) % 8))
                                    & 1,
                            ) << n;
                        }
                        raw as f32
                    } else if ieee {
                        f32::from_le_bytes(range(b, p, 4)?.try_into()?)
                    } else {
                        let raw = range(b, p, precision)?;
                        match (bits, signed) {
                            (8, false) => f32::from(raw[0]),
                            (8, true) => f32::from(raw[0] as i8),
                            (16, true) => f32::from(i16::from_le_bytes(raw.try_into()?)),
                            (32, false) => u32::from_le_bytes(raw.try_into()?) as f32,
                            _ => return Err("invalid IM float layout".into()),
                        }
                    };
                    numpy_float(f64::from(value))
                } else if precision == 0 {
                    let bit = x * bits;
                    let row = range(b, offset + y * rowsize, rowsize)?;
                    i64::from((row[bit / 8] >> (8 - bits - bit % 8)) & ((1 << bits) - 1))
                } else if mode == "I" {
                    i64::from(i32::from_le_bytes(range(b, p, 4)?.try_into()?))
                } else if precision == 2 {
                    let raw = range(b, p, 2)?.try_into()?;
                    i64::from(if mode == "I;16B" {
                        u16::from_be_bytes(raw)
                    } else {
                        u16::from_le_bytes(raw)
                    })
                } else {
                    i64::from(*b.get(p).ok_or("IM samples truncated")?)
                };
                values[((h - 1 - y) * w + x) * outch + c] = v;
            }
        }
    }
    Ok(Some(array(mode, w, h, outch, values)))
}
pub(super) fn pfm(b: &[u8]) -> Result<Array> {
    let mut at = 2;
    let mut words = vec![];
    while words.len() < 3 {
        while b.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1
        }
        let start = at;
        while b.get(at).is_some_and(|v| !v.is_ascii_whitespace()) {
            at += 1
        }
        if start == at {
            return Err("PFM header".into());
        }
        words.push(std::str::from_utf8(&b[start..at])?);
    }
    let w: usize = words[0].parse()?;
    let h: usize = words[1].parse()?;
    check_dimensions(w, h)?;
    let scale: f32 = words[2].parse()?;
    if scale == 0.0 || !scale.is_finite() {
        return Err("PFM scale".into());
    }
    at += 1;
    let ch = if b[1] == b'f' { 1 } else { 3 };
    let mut vals = zeroed(w * h * ch)?;
    for y in 0..h {
        for x in 0..w * ch {
            let raw = range(b, at + (y * w * ch + x) * 4, 4)?.try_into()?;
            let value = if scale < 0.0 {
                f32::from_le_bytes(raw)
            } else {
                f32::from_be_bytes(raw)
            };
            vals[(h - 1 - y) * w * ch + x] = numpy_float(value as f64);
        }
    }
    Ok(array(if ch == 1 { "F" } else { "RGB" }, w, h, ch, vals))
}

pub(super) fn icns(b: &[u8]) -> Result<Array> {
    use std::collections::BTreeMap;
    let declared = be32(b, 4)?;
    let mut chunks = BTreeMap::new();
    let mut at = 8usize;
    while at < declared {
        let code: [u8; 4] = range(b, at, 4)?.try_into()?;
        let length = be32(b, at + 4)?;
        if length < 8 {
            return Err("invalid ICNS block length".into());
        }
        let stop = at.checked_add(length).ok_or("ICNS block overflow")?;
        chunks.insert(code, (at + 8, length - 8));
        at = stop;
    }
    // Pillow orders by (logical width, logical height, scale), not pixel area.
    type IconSize = (usize, usize, &'static [&'static [u8; 4]]);
    let sizes: [IconSize; 12] = [
        (512, 2, &[b"ic10"]),
        (512, 1, &[b"ic09"]),
        (256, 2, &[b"ic14"]),
        (256, 1, &[b"ic08"]),
        (128, 2, &[b"ic13"]),
        (128, 1, &[b"ic07", b"it32", b"t8mk"]),
        (64, 1, &[b"icp6"]),
        (32, 2, &[b"ic12"]),
        (48, 1, &[b"ih32", b"h8mk"]),
        (32, 1, &[b"icp5", b"il32", b"l8mk"]),
        (16, 2, &[b"ic11"]),
        (16, 1, &[b"icp4", b"is32", b"s8mk"]),
    ];
    let &(logical, scale, codes) = sizes
        .iter()
        .filter(|(_, _, codes)| codes.iter().any(|code| chunks.contains_key(*code)))
        .max_by_key(|(logical, scale, _)| (*logical, *scale))
        .ok_or("No 32bit ICNS resources found")?;
    let width = logical * scale;
    check_dimensions(width, width)?;
    let count = width * width;
    let mut rgb = None;
    let mut alpha = None;
    let mut embedded = None;
    let mut rgb_padding = 255;
    for &code in codes {
        let Some(&(offset, length)) = chunks.get(code) else {
            continue;
        };
        if [b"s8mk", b"l8mk", b"h8mk", b"t8mk"].contains(&code) {
            alpha = Some(range(b, offset, count)?.to_vec());
            continue;
        }
        if [b"is32", b"il32", b"ih32", b"it32"].contains(&code) {
            let (offset, length) = if code == b"it32" {
                if range(b, offset, 4)? != [0; 4] {
                    return Err("Unknown ICNS signature, expecting 0x00000000".into());
                }
                (
                    offset + 4,
                    length.checked_sub(4).ok_or("ICNS short signature")?,
                )
            } else {
                (offset, length)
            };
            let mut values = zeroed(count * 3)?;
            if length == count * 3 {
                for (out, &value) in values.iter_mut().zip(range(b, offset, length)?) {
                    *out = i64::from(value);
                }
            } else {
                rgb_padding = 0;
                let mut position = offset;
                for channel in 0..3 {
                    let mut decoded = 0usize;
                    while decoded < count {
                        let control = *b.get(position).ok_or("ICNS short RLE channel")?;
                        position += 1;
                        let run = if control & 128 != 0 {
                            usize::from(control) - 125
                        } else {
                            usize::from(control) + 1
                        };
                        if run > count - decoded {
                            return Err("ICNS RLE channel overrun".into());
                        }
                        if control & 128 != 0 {
                            let value = *b.get(position).ok_or("ICNS short RLE sample")?;
                            position += 1;
                            for x in 0..run {
                                values[(decoded + x) * 3 + channel] = i64::from(value);
                            }
                        } else {
                            for (x, &value) in range(b, position, run)?.iter().enumerate() {
                                values[(decoded + x) * 3 + channel] = i64::from(value);
                            }
                            position += run;
                        }
                        decoded += run;
                    }
                }
            }
            rgb = Some(values);
        } else {
            let bytes = range(b, offset, length)?;
            if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                embedded = Some(super::png_decode(bytes)?);
            } else if bytes.starts_with(&[255, 79, 255, 81])
                || bytes.starts_with(b"\0\0\0\x0cjP  \r\n\x87\n")
            {
                #[cfg(feature = "image-array-native")]
                {
                    let image = super::decode(bytes)?;
                    let (w, h) = (image.shape[1], image.shape[0]);
                    let mut values = Vec::new();
                    values.try_reserve_exact(w * h * 4)?;
                    let ch = if image.shape.len() == 3 {
                        image.shape[2]
                    } else {
                        1
                    };
                    for pixel in image.values.chunks_exact(ch) {
                        match image.mode.as_str() {
                            "L" => values.extend([pixel[0], pixel[0], pixel[0], 255]),
                            "I;16" => {
                                let v = pixel[0].min(255);
                                values.extend([v, v, v, 255]);
                            }
                            "LA" => values.extend([pixel[0], pixel[0], pixel[0], pixel[1]]),
                            "RGB" => values.extend([pixel[0], pixel[1], pixel[2], 255]),
                            "RGBA" => values.extend_from_slice(pixel),
                            "P" | "PA" => {
                                return Err(
                                    "ICNS JPEG2000 palette conversion not admitted by original"
                                        .into(),
                                )
                            }
                            "CMYK" => {
                                let k = 255 - pixel[3];
                                for value in pixel.iter().take(3) {
                                    values.push(k - (value * k + 127) / 255);
                                }
                                values.push(255);
                            }
                            _ => return Err("unsupported ICNS JPEG2000 subimage mode".into()),
                        }
                    }
                    embedded = Some(array("RGBA", w, h, 4, values));
                }
                #[cfg(not(feature = "image-array-native"))]
                return Err("ICNS JPEG2000 requires native image codec feature".into());
            } else {
                return Err("unsupported ICNS embedded subimage format".into());
            }
        }
    }
    if let Some(image) = embedded {
        return icns_array_layout(image, 255);
    }
    let rgb = rgb.ok_or("ICNS resource has no RGB channel")?;
    if let Some(alpha) = alpha {
        let mut values = Vec::new();
        values.try_reserve_exact(count * 4)?;
        for (pixel, a) in rgb.chunks_exact(3).zip(alpha) {
            values.extend_from_slice(pixel);
            values.push(i64::from(a));
        }
        Ok(array("RGBA", width, width, 4, values))
    } else {
        icns_array_layout(array("RGB", width, width, 3, rgb), rgb_padding)
    }
}
fn icns_array_layout(mut image: Array, padding: i64) -> Result<Array> {
    if image.mode != "RGB" && image.mode != "RGBA" {
        return Err("unknown raw mode for given ICNS image mode".into());
    }
    if image.mode == "RGB" {
        // NumPy captures the initial RGBA byte format before ICNS load changes
        // its mode to RGB, leaving the original alpha-interleaved prefix.
        let count = image.values.len();
        let mut expanded = Vec::new();
        expanded.try_reserve_exact(count / 3 * 4)?;
        for pixel in image.values.chunks_exact(3) {
            expanded.extend_from_slice(pixel);
            expanded.push(padding);
        }
        expanded.truncate(count);
        image.values = expanded;
    }
    Ok(image)
}
