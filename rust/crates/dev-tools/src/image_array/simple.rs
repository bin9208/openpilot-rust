//! Additional installed Pillow readers that need no external codec dependency.
use super::{array, check_dimensions, numpy_float, Array, Result};
use std::io::Cursor;
fn range(b: &[u8], at: usize, len: usize) -> Result<&[u8]> {
    Ok(
        b.get(at..at.checked_add(len).ok_or("image offset overflow")?)
            .ok_or("truncated image")?,
    )
}
fn be32(b: &[u8], at: usize) -> Result<usize> {
    Ok(u32::from_be_bytes(range(b, at, 4)?.try_into()?) as usize)
}
fn le16(b: &[u8], at: usize) -> Result<usize> {
    Ok(u16::from_le_bytes(range(b, at, 2)?.try_into()?) as usize)
}
fn le32(b: &[u8], at: usize) -> Result<usize> {
    Ok(u32::from_le_bytes(range(b, at, 4)?.try_into()?) as usize)
}
fn packed(
    b: &[u8],
    w: usize,
    h: usize,
    depth: usize,
    stride: usize,
    invert: bool,
    scale: bool,
) -> Result<Vec<i64>> {
    let mut result = Vec::new();
    result.try_reserve_exact(w * h)?;
    for y in 0..h {
        let row = range(b, y * stride, stride)?;
        for x in 0..w {
            let mask = ((1u16 << depth) - 1) as u8;
            let mut v = i64::from((row[x * depth / 8] >> (8 - depth - x * depth % 8)) & mask);
            if invert {
                v = i64::from(mask) - v
            }
            if scale {
                v = v * 255 / i64::from(mask)
            }
            result.push(v);
        }
    }
    Ok(result)
}
pub(super) fn decode(b: &[u8]) -> Result<Option<Array>> {
    if b.starts_with(b"qoif") {
        return qoi(b).map(Some);
    }
    if b.starts_with(&[0xb1, 0x68, 0xde, 0x3a]) {
        let offset = le32(b, 4)?;
        return super::legacy::pcx(b.get(offset..).ok_or("DCX page offset")?).map(Some);
    }
    if matches!(b.get(..4), Some([12 | 40 | 64 | 108 | 124, 0, 0, 0])) {
        return dib(b).map(Some);
    }
    if b.starts_with(b"P7 332") {
        let mut lines = b.split_inclusive(|v| *v == b'\n');
        let mut at = lines.next().ok_or("XV header")?.len();
        for line in lines {
            at += line.len();
            if line.starts_with(b"#") {
                continue;
            }
            let size = std::str::from_utf8(line)?
                .split_ascii_whitespace()
                .take(2)
                .map(str::parse::<usize>)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            if size.len() != 2 {
                return Err("XV dimensions".into());
            }
            let (w, h) = (size[0], size[1]);
            check_dimensions(w, h)?;
            return Ok(Some(array(
                "P",
                w,
                h,
                1,
                range(b, at, w * h)?.iter().map(|v| i64::from(*v)).collect(),
            )));
        }
        return Err("XV header truncated".into());
    }
    if b.len() >= 20 && [1, 2].contains(&be32(b, 4)?) && be32(b, 0)? >= 20 {
        let header = be32(b, 0)?;
        let version = be32(b, 4)?;
        let (w, h, ch) = (be32(b, 8)?, be32(b, 12)?, be32(b, 16)?);
        check_dimensions(w, h)?;
        if ![1, 4].contains(&ch) || version == 2 && range(b, 20, 4)? != b"GIMP" {
            return Err("unsupported GIMP brush header".into());
        }
        return Ok(Some(array(
            if ch == 1 { "L" } else { "RGBA" },
            w,
            h,
            ch,
            range(b, header, w * h * ch)?
                .iter()
                .map(|v| i64::from(*v))
                .collect(),
        )));
    }
    if b.starts_with(b"DanM") || b.starts_with(b"LinS") {
        range(b, 0, 32)?;
        let check =
            (0..16).try_fold(0usize, |a, i| -> Result<usize> { Ok(a ^ le16(b, i * 2)?) })?;
        if check != 0 {
            return Err("bad MSP checksum".into());
        }
        let (w, h) = (le16(b, 4)?, le16(b, 6)?);
        check_dimensions(w, h)?;
        let stride = w.div_ceil(8);
        let data = if b.starts_with(b"DanM") {
            range(b, 32, stride * h)?.to_vec()
        } else {
            let mut data = Vec::new();
            data.try_reserve_exact(stride * h)?;
            let mut at = 32 + h * 2;
            for y in 0..h {
                let len = le16(b, 32 + y * 2)?;
                if len == 0 {
                    data.extend(std::iter::repeat_n(255, stride));
                    continue;
                }
                let row = range(b, at, len)?;
                at += len;
                let mut p = 0;
                while p < len {
                    let count = row[p] as usize;
                    p += 1;
                    if count == 0 {
                        let pair = range(row, p, 2)?;
                        p += 2;
                        data.extend(std::iter::repeat_n(pair[1], pair[0] as usize));
                    } else {
                        let end = (p + count).min(len);
                        data.extend_from_slice(&row[p..end]);
                        p += count;
                    }
                }
            }
            data
        };
        return Ok(Some(array(
            "1",
            w,
            h,
            1,
            packed(&data, w, h, 1, stride, false, false)?,
        )));
    }
    if b.starts_with(&[0x59, 0xa6, 0x6a, 0x95]) {
        return sun(b).map(Some);
    }
    if b.starts_with(b"/* XPM */") {
        return xpm(b).map(Some);
    }
    if b.iter()
        .copied()
        .skip_while(u8::is_ascii_whitespace)
        .take(7)
        .collect::<Vec<_>>()
        == b"#define"
    {
        return xbm(b).map(Some);
    }
    if b.get(..100).unwrap_or(b).contains(&b'\n') {
        let mut w = 0;
        let mut h = 0;
        let mut mode = false;
        let mut at = 0;
        while let Some(line) = b
            .get(at..)
            .and_then(|rest| rest.split_inclusive(|v| *v == b'\n').next())
        {
            if line.starts_with(&[12]) {
                check_dimensions(w, h)?;
                if !mode {
                    return Err("IMT pixel format".into());
                }
                return Ok(Some(array(
                    "L",
                    w,
                    h,
                    1,
                    range(b, at + 1, w * h)?
                        .iter()
                        .map(|v| i64::from(*v))
                        .collect(),
                )));
            }
            if line.len() > 100 {
                break;
            }
            at += line.len();
            if line.starts_with(b"*") {
                continue;
            }
            let Ok(text) = std::str::from_utf8(line) else {
                break;
            };
            let Some((key, value)) = text.trim_end_matches(['\r', '\n']).split_once(' ') else {
                break;
            };
            match key {
                "width" => w = value.parse()?,
                "height" => h = value.parse()?,
                "pixel" => mode = value == "n8",
                _ => {}
            }
        }
    }
    spider(b)
}
fn dib(b: &[u8]) -> Result<Array> {
    use image::ImageDecoder;
    let header = le32(b, 0)?;
    let mut file = Vec::new();
    let bits = if header == 12 {
        le16(b, 10)?
    } else {
        le16(b, 14)?
    };
    let colors = if header == 12 {
        1usize.checked_shl(bits as u32).unwrap_or(0)
    } else {
        let colors = le32(b, 32)?;
        if colors == 0 && bits <= 8 {
            1 << bits
        } else {
            colors
        }
    };
    let offset = 14
        + header
        + if bits <= 8 {
            colors * if header == 12 { 3 } else { 4 }
        } else {
            0
        };
    file.extend_from_slice(b"BM");
    file.extend_from_slice(&u32::try_from(b.len() + 14)?.to_le_bytes());
    file.extend_from_slice(&[0; 4]);
    file.extend_from_slice(&u32::try_from(offset)?.to_le_bytes());
    file.extend_from_slice(b);
    if let Some(array) = super::legacy::bmp(&file)? {
        return Ok(array);
    }
    let decoder = image::codecs::bmp::BmpDecoder::new_without_file_header(Cursor::new(b))?;
    let (w, h) = decoder.dimensions();
    check_dimensions(w as usize, h as usize)?;
    let image = image::DynamicImage::from_decoder(decoder)?;
    let rgb = image.into_rgb8();
    Ok(array(
        "RGB",
        w as usize,
        h as usize,
        3,
        rgb.into_raw().into_iter().map(i64::from).collect(),
    ))
}
fn spider(b: &[u8]) -> Result<Option<Array>> {
    if b.len() < 108 {
        return Ok(None);
    }
    for be in [true, false] {
        let mut head = Vec::new();
        for raw in b[..108].chunks_exact(4) {
            let raw = raw.try_into()?;
            head.push(if be {
                f32::from_be_bytes(raw)
            } else {
                f32::from_le_bytes(raw)
            });
        }
        if [0, 1, 4, 11, 12, 21, 22]
            .iter()
            .any(|&i| !head[i].is_finite() || head[i].fract() != 0.0)
            || head[4] != 1.0
            || head[21] != head[12] * head[22]
            || head[21] <= 0.0
        {
            continue;
        }
        let (w, h) = (head[11] as usize, head[1] as usize);
        check_dimensions(w, h)?;
        let offset = head[21] as usize
            * if head[23] > 0.0 && head[26] == 0.0 {
                2
            } else {
                1
            };
        if head[23] < 0.0 || head[23] == 0.0 && head[26] > 0.0 {
            return Err("inconsistent Spider stack header".into());
        }
        let mut values = Vec::new();
        values.try_reserve_exact(w * h)?;
        for raw in range(b, offset, w * h * 4)?.chunks_exact(4) {
            let raw = raw.try_into()?;
            values.push(numpy_float(f64::from(if be {
                f32::from_be_bytes(raw)
            } else {
                f32::from_le_bytes(raw)
            })));
        }
        return Ok(Some(array("F", w, h, 1, values)));
    }
    Ok(None)
}
fn sun(b: &[u8]) -> Result<Array> {
    let (w, h, depth, kind, palette, len) = (
        be32(b, 4)?,
        be32(b, 8)?,
        be32(b, 12)?,
        be32(b, 20)?,
        be32(b, 24)?,
        be32(b, 28)?,
    );
    check_dimensions(w, h)?;
    if ![1, 4, 8, 24, 32].contains(&depth)
        || ![0, 1, 2, 3, 4, 5].contains(&kind)
        || len > 1024
        || len > 0 && palette != 1
    {
        return Err("unsupported Sun raster header".into());
    }
    let stride = if kind == 2 {
        (w * depth).div_ceil(8)
    } else {
        (w * depth).div_ceil(16) * 2
    };
    let offset = 32 + len;
    let mut decoded = Vec::new();
    let data = if kind == 2 {
        let mut at = offset;
        decoded.try_reserve_exact(stride * h)?;
        while decoded.len() < stride * h {
            let value = *b.get(at).ok_or("Sun RLE truncated")?;
            at += 1;
            if value != 128 {
                decoded.push(value)
            } else {
                let count = *b.get(at).ok_or("Sun RLE count")?;
                at += 1;
                if count == 0 {
                    decoded.push(128)
                } else {
                    let v = *b.get(at).ok_or("Sun RLE sample")?;
                    at += 1;
                    decoded.extend(std::iter::repeat_n(v, usize::from(count) + 1));
                }
            }
        }
        decoded.as_slice()
    } else {
        range(b, offset, stride * h)?
    };
    if depth <= 8 {
        let mode = if depth == 1 {
            "1"
        } else if len > 0 {
            "P"
        } else {
            "L"
        };
        return Ok(array(
            mode,
            w,
            h,
            1,
            packed(
                data,
                w,
                h,
                depth,
                stride,
                depth == 1,
                depth == 4 && len == 0,
            )?,
        ));
    }
    let mut values = Vec::new();
    values.try_reserve_exact(w * h * 3)?;
    for y in 0..h {
        let row = range(data, y * stride, stride)?;
        for x in 0..w {
            let pixel = range(row, x * (depth / 8), 3)?;
            if kind == 3 {
                values.extend(pixel.iter().map(|v| i64::from(*v)))
            } else {
                values.extend(pixel.iter().rev().map(|v| i64::from(*v)))
            }
        }
    }
    Ok(array("RGB", w, h, 3, values))
}
fn xbm(b: &[u8]) -> Result<Array> {
    let text = std::str::from_utf8(b)?;
    let mut w = 0;
    let mut h = 0;
    for line in text.lines().take(4) {
        let words = line.split_ascii_whitespace().collect::<Vec<_>>();
        if words.len() >= 3 && words[0] == "#define" {
            if words[1].ends_with("_width") {
                w = words[2].parse()?
            } else if words[1].ends_with("_height") {
                h = words[2].parse()?
            }
        }
    }
    check_dimensions(w, h)?;
    let data = text.split_once('{').ok_or("XBM data")?.1;
    let mut bytes = Vec::new();
    for value in data
        .split(|v: char| !v.is_ascii_alphanumeric())
        .filter(|v| v.starts_with("0x"))
    {
        bytes.push(u8::from_str_radix(&value[2..], 16)?);
    }
    let stride = w.div_ceil(8);
    range(&bytes, 0, stride * h)?;
    let mut values = Vec::new();
    values.try_reserve_exact(w * h)?;
    for y in 0..h {
        for x in 0..w {
            values.push(i64::from((bytes[y * stride + x / 8] >> (x % 8)) & 1));
        }
    }
    Ok(array("1", w, h, 1, values))
}
fn xpm(b: &[u8]) -> Result<Array> {
    let mut lines = b.split(|v| *v == b'\n');
    let header = lines
        .find(|line| line.starts_with(b"\""))
        .ok_or("XPM header")?;
    let header = std::str::from_utf8(header)?.trim_start_matches('"');
    let fields = header
        .split_ascii_whitespace()
        .take(4)
        .map(|v| v.trim_end_matches([',', '"']).parse::<usize>())
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if fields.len() != 4 {
        return Err("XPM header fields".into());
    }
    let (w, h, count, bpp) = (fields[0], fields[1], fields[2], fields[3]);
    check_dimensions(w, h)?;
    if bpp == 0 {
        return Err("XPM pixel width".into());
    }
    let mut palette: Vec<(Vec<u8>, [u8; 3])> = Vec::new();
    for _ in 0..count {
        let line = lines.next().ok_or("XPM palette truncated")?;
        let key = range(line, 1, bpp)?.to_vec();
        let words = std::str::from_utf8(line.get(1 + bpp..).ok_or("XPM palette")?)?
            .trim_end_matches(['\r', ',', '"'])
            .split_ascii_whitespace()
            .collect::<Vec<_>>();
        let color = words
            .chunks(2)
            .find(|v| v[0] == "c")
            .and_then(|v| v.get(1))
            .ok_or("XPM color key")?;
        if *color == "None" {
            continue;
        }
        let hex = color
            .strip_prefix('#')
            .ok_or("unsupported XPM named color")?;
        let value = u32::from_str_radix(hex, 16)?;
        palette.push((key, [(value >> 16) as u8, (value >> 8) as u8, value as u8]));
    }
    let ch = if count > 256 { 3 } else { 1 };
    let mut values = Vec::new();
    values.try_reserve_exact(w * h * ch)?;
    for line in lines {
        if values.len() >= w * h * ch {
            break;
        }
        let Some(first) = line.iter().position(|v| *v == b'"') else {
            continue;
        };
        let Some(last) = line.iter().rposition(|v| *v == b'"') else {
            continue;
        };
        if first == last {
            continue;
        }
        for key in line[first + 1..last].chunks(bpp) {
            let index = palette
                .iter()
                .position(|(k, _)| k == key)
                .ok_or("unknown XPM pixel key")?;
            if ch == 1 {
                values.push(index as i64)
            } else {
                values.extend(palette[index].1.map(i64::from));
            }
        }
    }
    if values.len() < w * h * ch {
        return Err("XPM pixel data truncated".into());
    }
    values.truncate(w * h * ch);
    Ok(array(if ch == 1 { "P" } else { "RGB" }, w, h, ch, values))
}
fn qoi(b: &[u8]) -> Result<Array> {
    let (w, h) = (be32(b, 4)?, be32(b, 8)?);
    check_dimensions(w, h)?;
    let ch = *b.get(12).ok_or("QOI header")? as usize;
    if ![3, 4].contains(&ch) {
        return Err("unsupported QOI channels".into());
    }
    let mut seen = [[0u8; 4]; 64];
    let mut pixel = [0u8, 0, 0, 255];
    let mut at = 14;
    let mut values = Vec::new();
    values.try_reserve_exact(w * h * ch)?;
    while values.len() < w * h * ch {
        let control = *b.get(at).ok_or("QOI pixels truncated")?;
        at += 1;
        match control {
            254 => {
                pixel[..3].copy_from_slice(range(b, at, 3)?);
                at += 3;
            }
            255 => {
                pixel.copy_from_slice(range(b, at, 4)?);
                at += 4;
            }
            v => match v >> 6 {
                0 => pixel = seen[(v & 63) as usize],
                1 => {
                    for (c, shift) in [4, 2, 0].into_iter().enumerate() {
                        pixel[c] = pixel[c].wrapping_add(((v >> shift) & 3).wrapping_sub(2));
                    }
                }
                2 => {
                    let second = *b.get(at).ok_or("QOI luma truncated")?;
                    at += 1;
                    let green = (v & 63).wrapping_sub(32);
                    pixel[0] = pixel[0]
                        .wrapping_add(green)
                        .wrapping_add((second >> 4).wrapping_sub(8));
                    pixel[1] = pixel[1].wrapping_add(green);
                    pixel[2] = pixel[2]
                        .wrapping_add(green)
                        .wrapping_add((second & 15).wrapping_sub(8));
                }
                _ => {
                    for _ in 0..usize::from((v & 63) + 1) {
                        values.extend(pixel[..ch].iter().map(|v| i64::from(*v)));
                    }
                    continue;
                }
            },
        }
        let hash = (usize::from(pixel[0]) * 3
            + usize::from(pixel[1]) * 5
            + usize::from(pixel[2]) * 7
            + usize::from(pixel[3]) * 11)
            % 64;
        seen[hash] = pixel;
        values.extend(pixel[..ch].iter().map(|v| i64::from(*v)));
    }
    values.truncate(w * h * ch);
    Ok(array(
        if ch == 3 { "RGB" } else { "RGBA" },
        w,
        h,
        ch,
        values,
    ))
}
