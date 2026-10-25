//! Remaining legacy readers, ported from Pillow 12.3.0 (see PILLOW-LICENSE).
//! Kept separate until the independent original-array gate has passed.
use super::{array, check_dimensions, numpy_float, zeroed, Array, Result};
use std::{collections::BTreeMap, io::Read};
#[path = "animation.rs"]
mod animation;
#[path = "photo_ycc.rs"]
mod photo_ycc;
#[path = "textures.rs"]
mod textures;
fn range(b: &[u8], at: usize, len: usize) -> Result<&[u8]> {
    Ok(
        b.get(at..at.checked_add(len).ok_or("image offset overflow")?)
            .ok_or("truncated image")?,
    )
}
fn le16(b: &[u8], at: usize) -> Result<usize> {
    Ok(u16::from_le_bytes(range(b, at, 2)?.try_into()?) as usize)
}
fn be16(b: &[u8], at: usize) -> Result<usize> {
    Ok(u16::from_be_bytes(range(b, at, 2)?.try_into()?) as usize)
}
fn le32(b: &[u8], at: usize) -> Result<usize> {
    Ok(u32::from_le_bytes(range(b, at, 4)?.try_into()?) as usize)
}
fn be32(b: &[u8], at: usize) -> Result<usize> {
    Ok(u32::from_be_bytes(range(b, at, 4)?.try_into()?) as usize)
}
pub(super) fn decode(b: &[u8]) -> Result<Option<Array>> {
    if b.starts_with(b"8BPS") {
        return psd(b).map(Some);
    }
    if b.starts_with(b"BLP1") || b.starts_with(b"BLP2") {
        return textures::blp(b).map(Some);
    }
    if b.starts_with(b"FTEX") {
        return textures::ftex(b).map(Some);
    }
    if b.starts_with(b"SIMPLE") {
        return fits(b).map(Some);
    }
    if b.starts_with(&[0, 0, 0, 0, 0, 0, 0, 4]) {
        return mcidas(b).map(Some);
    }
    if b.starts_with(&[0x80, 0xe8, 0, 0]) {
        let (w, h) = (le16(b, 418)?, le16(b, 416)?);
        check_dimensions(w, h)?;
        if (le16(b, 424)?, le16(b, 426)?) != (14, 2) {
            return Err("unsupported Pixar mode".into());
        }
        return Ok(Some(array(
            "RGB",
            w,
            h,
            3,
            range(b, 1024, w * h * 3)?
                .iter()
                .copied()
                .map(i64::from)
                .collect(),
        )));
    }
    // CUR and uncompressed TGA share a prefix. Pillow continues probing when
    // the cursor directory/bitmap offset cannot even supply a BMP header size.
    // Once a bitmap header is present, retain its decode errors.
    if b.starts_with(&[0, 0, 2, 0]) && cur_admitted(b) {
        return cur(b).map(Some);
    }
    if matches!(b.get(4..6), Some([0x11 | 0x12, 0xaf])) {
        return animation::fli(b).map(Some);
    }
    if b.starts_with(b"%!") || b.starts_with(&[0xc5, 0xd0, 0xd3, 0xc6]) {
        return eps(b).map(Some);
    }
    if b.get(2048..2052) == Some(b"PCD_") {
        return pcd(b).map(Some);
    }
    if b.starts_with(&[0x1c]) {
        return iptc(b).map(Some);
    }
    Ok(None)
}
fn mcidas(b: &[u8]) -> Result<Array> {
    range(b, 0, 256)?;
    let (w, h, depth) = (be32(b, 36)?, be32(b, 32)?, be32(b, 40)?);
    check_dimensions(w, h)?;
    let mode = match depth {
        1 => "L",
        2 => "I;16B",
        4 => "I",
        _ => return Err("unsupported McIDAS depth".into()),
    };
    let skip = be32(b, 56)?;
    let at = be32(b, 132)?
        .checked_add(skip)
        .ok_or("McIDAS offset overflow")?;
    let stride = w
        .checked_mul(depth)
        .and_then(|v| v.checked_mul(be32(b, 52).ok()?))
        .and_then(|v| v.checked_add(skip))
        .ok_or("McIDAS stride overflow")?;
    let mut values = Vec::new();
    values.try_reserve_exact(w * h)?;
    for y in 0..h {
        for p in range(b, at + y * stride, w * depth)?.chunks_exact(depth) {
            values.push(match depth {
                1 => i64::from(p[0]),
                2 => i64::from(u16::from_be_bytes(p.try_into()?)),
                _ => i64::from(i32::from_be_bytes(p.try_into()?)),
            });
        }
    }
    Ok(array(mode, w, h, 1, values))
}
fn psd(b: &[u8]) -> Result<Array> {
    if be16(b, 4)? != 1 {
        return Err("PSD version".into());
    }
    let (provided, h, w, bits, kind) = (
        be16(b, 12)?,
        be32(b, 14)?,
        be32(b, 18)?,
        be16(b, 22)?,
        be16(b, 24)?,
    );
    check_dimensions(w, h)?;
    let (mode, ch) = match (kind, bits) {
        (0, 1) => ("1", 1),
        (0 | 1 | 7 | 8, 8) => ("L", 1),
        (2, 8) => ("P", 1),
        (3, 8) if provided == 4 => ("RGBA", 4),
        (3, 8) => ("RGB", 3),
        (4, 8) => ("CMYK", 4),
        (9, 8) => ("LAB", 3),
        _ => return Err("unsupported PSD mode".into()),
    };
    if provided < ch {
        return Err("not enough PSD channels".into());
    }
    let mut at = 26usize;
    for _ in 0..3 {
        let n = be32(b, at)?;
        at = at
            .checked_add(4)
            .and_then(|v| v.checked_add(n))
            .ok_or("PSD offset overflow")?;
        range(b, at, 0)?;
    }
    let compression = be16(b, at)?;
    at += 2;
    if compression > 1 {
        return Err("unsupported PSD compression".into());
    }
    let mut counts = Vec::new();
    if compression == 1 {
        for _ in 0..ch * h {
            counts.push(be16(b, at)?);
            at += 2;
        }
    }
    let mut values = zeroed(w * h * ch)?;
    for c in 0..ch {
        let raw = if compression == 0 {
            let n = if bits == 1 { w.div_ceil(8) * h } else { w * h };
            let raw = range(b, at, n)?.to_vec();
            at += w * h;
            raw
        } else {
            let mut raw = Vec::new();
            for y in 0..h {
                let n = counts[c * h + y];
                raw.extend(packbits(range(b, at, n)?)?);
                at += n;
            }
            raw
        };
        let stride = if bits == 1 { w.div_ceil(8) } else { w };
        for y in 0..h {
            for x in 0..w {
                let mut v = if bits == 1 {
                    i64::from((range(&raw, y * stride + x / 8, 1)?[0] >> (7 - x % 8)) & 1)
                } else {
                    i64::from(range(&raw, y * w + x, 1)?[0])
                };
                if mode == "CMYK" {
                    v = 255 - v;
                }
                if mode == "LAB" && c > 0 {
                    v ^= 128;
                }
                values[(y * w + x) * ch + c] = v;
            }
        }
    }
    Ok(array(mode, w, h, ch, values))
}
fn packbits(b: &[u8]) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    let mut at = 0;
    while at < b.len() {
        let n = b[at];
        at += 1;
        match n {
            0..=127 => {
                result.extend_from_slice(range(b, at, n as usize + 1)?);
                at += n as usize + 1;
            }
            128 => {}
            _ => {
                let v = range(b, at, 1)?[0];
                at += 1;
                result.resize(result.len() + 257 - n as usize, v);
            }
        }
    }
    Ok(result)
}
fn fits(b: &[u8]) -> Result<Array> {
    let mut headers = BTreeMap::<String, String>::new();
    let mut at = 0usize;
    loop {
        let card = range(b, at, 80)?;
        at += 80;
        // FITS cards are byte records. Comments need not be UTF-8 and a
        // multibyte sequence may cross the eight-byte keyword boundary.
        let keyword = String::from_utf8_lossy(&card[..8]);
        let key = keyword.trim();
        if key == "END" {
            at = at.div_ceil(2880) * 2880;
            let num = |key: &str| -> Result<i64> {
                Ok(headers.get(key).ok_or("missing FITS header")?.parse()?)
            };
            let gzip = headers.get("XTENSION").is_some_and(|v| v == "'BINTABLE'")
                && headers.get("ZIMAGE").is_some_and(|v| v == "T")
                && headers.get("ZCMPTYPE").is_some_and(|v| v == "'GZIP_1  '");
            if num("NAXIS")? == 0 && !gzip {
                continue;
            }
            let prefix = if gzip { "Z" } else { "" };
            let axes = num(&format!("{prefix}NAXIS"))?;
            let a = usize::try_from(num(&format!("{prefix}NAXIS1"))?)?;
            let (w, h) = if axes == 1 {
                (1, a)
            } else {
                (a, usize::try_from(num(&format!("{prefix}NAXIS2"))?)?)
            };
            check_dimensions(w, h)?;
            let bits = num(&format!("{prefix}BITPIX"))?;
            let (mode, depth) = match bits {
                8 => ("L", 1),
                16 => ("I;16", 2),
                32 => ("I", 4),
                -32 | -64 => ("F", 4),
                _ => return Err("unsupported FITS depth".into()),
            };
            // Pillow reads one 80-byte record to distinguish another header
            // unit from data, then subtracts 80 from tell(). A short final
            // record therefore points into the preceding header padding.
            // Retain that observable offset, including subsequent extensions.
            let mut in_header = false;
            loop {
                let end = at.saturating_add(80).min(b.len());
                let card = b.get(at..end).ok_or("FITS data offset")?;
                if card.is_empty() {
                    return Err("truncated FITS file".into());
                }
                at = end;
                let keyword = card[..card.len().min(8)].trim_ascii();
                if matches!(keyword, b"SIMPLE" | b"XTENSION") {
                    in_header = true;
                } else if !in_header {
                    at = at.checked_sub(80).ok_or("FITS data offset underflow")?;
                    break;
                } else if keyword == b"END" {
                    at = at.div_ceil(2880) * 2880;
                    in_header = false;
                }
            }
            // Pillow's raw decoder uses native little-endian samples, including
            // four-byte F samples even when the FITS BITPIX card says -64.
            let data = if gzip {
                // Pillow applies its NAXIS shape rule to the storage table too:
                // NAXIS=1 has shape (1, NAXIS1), without requiring NAXIS2.
                let storage_axes = num("NAXIS")?;
                let storage_pixels = if storage_axes == 0 {
                    0
                } else if storage_axes == 1 {
                    num("NAXIS1")?
                } else {
                    num("NAXIS1")?
                        .checked_mul(num("NAXIS2")?)
                        .ok_or("FITS storage dimensions overflow")?
                };
                let skip = usize::try_from(
                    storage_pixels
                        .checked_mul(num("BITPIX")?.div_euclid(8))
                        .ok_or("FITS storage byte count overflow")?,
                )?;
                at = at.checked_add(skip).ok_or("FITS offset overflow")?;
                let mut decoded = Vec::new();
                flate2::read::GzDecoder::new(range(
                    b,
                    at,
                    b.len().checked_sub(at).ok_or("FITS offset")?,
                )?)
                .take((w * h * 4) as u64)
                .read_to_end(&mut decoded)?;
                if bits < 0 {
                    return Err("truncated FITS gzip floating samples".into());
                }
                let mut raw = Vec::new();
                for word in range(&decoded, 0, w * h * 4)?.chunks_exact(4) {
                    raw.extend_from_slice(&word[4 - depth..]);
                }
                raw
            } else {
                range(b, at, w * h * depth)?.to_vec()
            };
            let mut values = Vec::new();
            values.try_reserve_exact(w * h)?;
            for y in (0..h).rev() {
                for p in range(&data, y * w * depth, w * depth)?.chunks_exact(depth) {
                    values.push(match mode {
                        "L" => i64::from(p[0]),
                        "I;16" => i64::from(u16::from_le_bytes(p.try_into()?)),
                        "I" => i64::from(i32::from_le_bytes(p.try_into()?)),
                        _ => numpy_float(f32::from_le_bytes(p.try_into()?) as f64),
                    });
                }
            }
            return Ok(array(mode, w, h, 1, values));
        }
        let field = String::from_utf8_lossy(&card[8..]);
        let value = field
            .split('/')
            .next()
            .unwrap_or("")
            .trim()
            .trim_start_matches('=')
            .trim();
        if headers.is_empty() && (key != "SIMPLE" || value != "T") {
            return Err("invalid FITS primary header".into());
        }
        headers.insert(key.to_owned(), value.to_owned());
    }
}
fn cur_admitted(b: &[u8]) -> bool {
    let Ok(count) = le16(b, 4) else { return false };
    let mut selected = None;
    for i in 0..count {
        let Some(entry) = b.get(6 + i * 16..6 + (i + 1) * 16) else {
            return false;
        };
        if selected.is_none_or(|old: &[u8]| entry[0] > old[0] && entry[1] > old[1]) {
            selected = Some(entry);
        }
    }
    selected
        .and_then(|entry| le32(entry, 12).ok())
        .is_some_and(|offset| b.get(offset..offset + 4).is_some())
}
fn cur(b: &[u8]) -> Result<Array> {
    let mut selected = None;
    for i in 0..le16(b, 4)? {
        let entry = range(b, 6 + i * 16, 16)?;
        if selected.is_none_or(|old: &[u8]| entry[0] > old[0] && entry[1] > old[1]) {
            selected = Some(entry);
        }
    }
    let entry = selected.ok_or("no cursors")?;
    let offset = le32(entry, 12)?;
    let mut dib = range(b, offset, b.len().checked_sub(offset).ok_or("CUR offset")?)?.to_vec();
    let header = le32(&dib, 0)?;
    let height = if header == 12 {
        le16(&dib, 6)?
    } else {
        le32(&dib, 8)?
    };
    let w = if header == 12 {
        le16(&dib, 4)?
    } else {
        le32(&dib, 4)?
    };
    let h = height / 2;
    check_dimensions(w, h)?;
    let original = dib.clone();
    // The bitmap header includes the AND mask in its height, but true-color
    // masks are packed at one bit per pixel and are never decoded by Pillow.
    if header == 12 {
        dib[6..8].copy_from_slice(&u16::try_from(h)?.to_le_bytes());
    } else {
        dib[8..12].copy_from_slice(&u32::try_from(h)?.to_le_bytes());
    }
    let half = super::simple::decode(&dib)?.ok_or("CUR DIB image")?;
    if matches!(half.mode.as_str(), "1" | "L") {
        let full = super::simple::decode(&original)?.ok_or("CUR DIB image")?;
        let mut values = Vec::new();
        values.try_reserve_exact(w * h * 2)?;
        for i in 0..w * h {
            let scale = if full.mode == "1" { 255 } else { 1 };
            let mask = 255 - full.values[i] * scale;
            let xor = full.values[w * h + i] * scale;
            values.push((xor * mask + 127) / 255);
            values.push(mask);
        }
        return Ok(array("LA", w, h, 2, values));
    }
    if le16(&dib, if header == 12 { 10 } else { 14 })? == 32 {
        let at = header;
        let stride = w * 4;
        let mut values = Vec::new();
        values.try_reserve_exact(w * h * 4)?;
        for y in (0..h).rev() {
            for p in range(&dib, at + y * stride, stride)?.chunks_exact(4) {
                values.extend([
                    i64::from(p[2]),
                    i64::from(p[1]),
                    i64::from(p[0]),
                    i64::from(p[3]),
                ]);
            }
        }
        return Ok(array("RGBA", w, h, 4, values));
    }
    super::simple::decode(&dib)?.ok_or_else(|| "CUR DIB image".into())
}
#[derive(Debug)]
struct EpsHeader {
    bbox: [i64; 4],
    width: usize,
    height: usize,
    mode: &'static str,
}
fn eps_header(text: &[u8]) -> Result<EpsHeader> {
    let (mut at, mut header, mut trailer, mut atend) = (0usize, true, false, false);
    let (mut adobe, mut bbox_comment) = (false, false);
    let mut bbox: Option<Vec<i64>> = None;
    let mut descriptor = None;
    while at < text.len() {
        let start = at;
        while at < text.len() && !matches!(text[at], b'\n' | b'\r') {
            at += 1;
        }
        let line = &text[start..at];
        at += usize::from(at < text.len());
        if line.is_empty() {
            continue;
        }
        if line[0] == b'%' && line.len() > 255 {
            return Err("EPS comment exceeds 255 bytes".into());
        }
        if header && (line[0] != b'%' || line.starts_with(b"%%EndComments")) {
            if !adobe || !bbox_comment {
                return Err("EPS required header comment missing".into());
            }
            header = false;
            continue;
        }
        if header || (trailer && atend) {
            if header && line.starts_with(b"%!PS-Adobe") {
                adobe = true;
            }
            if let Some(value) = line.strip_prefix(b"%%BoundingBox:") {
                bbox_comment = true;
                let value = std::str::from_utf8(value)?.trim();
                if value == "(atend)" {
                    atend = true;
                } else if bbox.as_ref().is_none_or(Vec::is_empty) || (trailer && atend) {
                    let numbers = value
                        .split_whitespace()
                        .map(str::parse::<f64>)
                        .collect::<std::result::Result<Vec<_>, _>>();
                    if let Ok(numbers) = numbers {
                        if numbers.iter().all(|v| {
                            v.is_finite()
                                && (-9223372036854775808.0..9223372036854775808.0).contains(v)
                        }) {
                            bbox = Some(numbers.into_iter().map(|v| v as i64).collect());
                        }
                    }
                }
            }
        }
        if !header {
            if let Some(value) = line.strip_prefix(b"%ImageData:") {
                if descriptor.is_none() {
                    let text = std::str::from_utf8(value)?;
                    let numbers = text
                        .split_whitespace()
                        .take(4)
                        .map(str::parse::<i64>)
                        .collect::<std::result::Result<Vec<_>, _>>()?;
                    if numbers.len() != 4 {
                        return Err("EPS ImageData descriptor truncated".into());
                    }
                    let mode = match (numbers[2], numbers[3]) {
                        (1, _) => "1",
                        (8, 1) => "L",
                        (8, 2) => "LAB",
                        (8, 3) => "RGB",
                        (8, 4) => "CMYK",
                        // Pillow stops header scanning for other bit depths.
                        (8, _) => return Err("EPS ImageData mode invalid".into()),
                        _ => break,
                    };
                    descriptor = Some((
                        usize::try_from(numbers[0])?,
                        usize::try_from(numbers[1])?,
                        mode,
                    ));
                }
            } else if line.starts_with(b"%%EOF") {
                break;
            } else if line.starts_with(b"%%Trailer") {
                trailer = true;
            } else if let Some(value) = line.strip_prefix(b"%%BeginBinary:") {
                let count = std::str::from_utf8(value)?.trim().parse::<usize>()?;
                at = at.checked_add(count).ok_or("EPS binary offset overflow")?;
            }
        }
    }
    if header && (!adobe || !bbox_comment) {
        return Err("EPS required header comment missing".into());
    }
    let bbox = bbox.ok_or("EPS bounding box missing")?;
    let bbox: [i64; 4] = bbox
        .get(..4)
        .ok_or("EPS bounding box truncated")?
        .try_into()?;
    let (width, height, mode) = if let Some(value) = descriptor {
        value
    } else {
        (
            usize::try_from(bbox[2].checked_sub(bbox[0]).ok_or("EPS width overflow")?)?,
            usize::try_from(bbox[3].checked_sub(bbox[1]).ok_or("EPS height overflow")?)?,
            "RGB",
        )
    };
    let extent_x = bbox[2].checked_sub(bbox[0]).ok_or("EPS width overflow")?;
    let extent_y = bbox[3].checked_sub(bbox[1]).ok_or("EPS height overflow")?;
    if extent_x == 0 || extent_y == 0 {
        return Err("EPS zero bounding box extent".into());
    }
    bbox[0].checked_neg().ok_or("EPS x origin overflow")?;
    bbox[1].checked_neg().ok_or("EPS y origin overflow")?;
    check_dimensions(width, height)?;
    Ok(EpsHeader {
        bbox,
        width,
        height,
        mode,
    })
}
fn eps(b: &[u8]) -> Result<Array> {
    use std::{io::Write, process::Command};
    let text = if b.starts_with(&[0xc5, 0xd0, 0xd3, 0xc6]) {
        range(b, le32(b, 4)?, le32(b, 8)?)?
    } else {
        b
    };
    let EpsHeader {
        bbox,
        width: w,
        height: h,
        mode,
    } = eps_header(text)?;
    let mut input = tempfile::NamedTempFile::new()?;
    input.write_all(b)?;
    input.flush()?;
    let output = tempfile::NamedTempFile::new()?;
    let status = Command::new("gs")
        .arg("-q")
        .arg(format!("-g{w}x{h}"))
        .arg(format!(
            "-r{:.6}x{:.6}",
            72.0 * w as f64 / (bbox[2] - bbox[0]) as f64,
            72.0 * h as f64 / (bbox[3] - bbox[1]) as f64
        ))
        .args(["-dBATCH", "-dNOPAUSE", "-dSAFER", "-sDEVICE=pnmraw"])
        .arg(format!("-sOutputFile={}", output.path().display()))
        .arg("-c")
        .arg(format!("{} {} translate", -bbox[0], -bbox[1]))
        .arg("-f")
        .arg(input.path())
        .args(["-c", "showpage"])
        .output()?;
    if !status.status.success() {
        return Err(format!(
            "Ghostscript failed: {}",
            String::from_utf8_lossy(&status.stderr)
        )
        .into());
    }
    let image = super::decode(&std::fs::read(output.path())?)?;
    // NumPy requests raw L before loading an initial mode-1 EPS. Ghostscript
    // may replace that mode with L; the other initial modes require their own
    // raw encoder and still fail when the rendered mode differs.
    if image.mode != mode && !(mode == "1" && image.mode == "L") {
        return Err("EPS raw encoder mode differs after Ghostscript rendering".into());
    }
    Ok(image)
}
fn iptc(b: &[u8]) -> Result<Array> {
    let mut at = 0;
    let mut fields = BTreeMap::new();
    let mut pixels = Vec::new();
    while at < b.len() {
        let head = range(b, at, 5)?;
        if head == [0; 5] {
            break;
        }
        if head[0] != 0x1c || !matches!(head[1], 1..=9 | 240) {
            return Err("invalid IPTC field".into());
        }
        at += 5;
        let size = match head[3] {
            0..=127 => usize::from(u16::from_be_bytes([head[3], head[4]])),
            128 => 0,
            129..=132 => {
                let n = (head[3] - 128) as usize;
                let mut value = 0usize;
                for v in range(b, at, n)? {
                    value = (value << 8) | usize::from(*v);
                }
                at += n;
                value
            }
            _ => return Err("invalid IPTC field length".into()),
        };
        let data = range(b, at, size)?;
        at += size;
        if head[1..3] == [8, 10] {
            pixels.extend_from_slice(data);
        } else if pixels.is_empty() {
            fields.insert((head[1], head[2]), data);
        } else {
            break;
        }
    }
    let field = |id| -> Result<&[u8]> { Ok(*fields.get(&(3, id)).ok_or("missing IPTC field")?) };
    let num = |id| -> Result<usize> {
        let mut v = 0usize;
        for n in field(id)?.iter().rev().take(4).rev() {
            v = (v << 8) | usize::from(*n);
        }
        Ok(v)
    };
    let (w, h) = (num(20)?, num(30)?);
    check_dimensions(w, h)?;
    let layer = field(60)?;
    let (mode, ch, band) = match range(layer, 0, 2)? {
        [1, 0] => ("L", 1, None),
        [3, 1..=255] => (
            "RGB",
            3,
            Some(fields.get(&(3, 65)).map_or(1, |v| v[0]) as usize - 1),
        ),
        [4, 1..=255] => (
            "CMYK",
            4,
            Some(fields.get(&(3, 65)).map_or(1, |v| v[0]) as usize - 1),
        ),
        _ => return Err("unsupported IPTC layers".into()),
    };
    let image = match num(120)? {
        1 => array(
            "L",
            w,
            h,
            1,
            range(&pixels, 0, w * h)?
                .iter()
                .copied()
                .map(i64::from)
                .collect(),
        ),
        5 => super::decode(&pixels)?,
        _ => return Err("unsupported IPTC compression".into()),
    };
    if let Some(band) = band {
        if image.mode != "L" || band >= ch {
            return Err("IPTC component image mode".into());
        }
        let mut values = zeroed(w * h * ch)?;
        for (i, v) in image.values.into_iter().enumerate() {
            values[i * ch + band] = v;
        }
        Ok(array(mode, w, h, ch, values))
    } else if image.mode == mode {
        Ok(image)
    } else {
        Err("IPTC raw mode changed".into())
    }
}

fn pcd(b: &[u8]) -> Result<Array> {
    const W: usize = 768;
    const H: usize = 512;
    let orientation = range(b, 2048 + 1538, 1)?[0] & 3;
    let mut values = zeroed(W * H * 3)?;
    for pair in 0..H / 2 {
        let data = range(b, 96 * 2048 + pair * W * 3, W * 3)?;
        for row in 0..2 {
            for x in 0..W {
                let y = pair * 2 + row;
                let l = i32::from(photo_ycc::L[data[row * W + x] as usize]);
                let cb = data[(x + 4 * W) / 2] as usize;
                let cr = data[(x + 5 * W) / 2] as usize;
                let rgb = [
                    l + i32::from(photo_ycc::CR[cr]),
                    l + i32::from(photo_ycc::GR[cr]) + i32::from(photo_ycc::GB[cb]),
                    l + i32::from(photo_ycc::CB[cb]),
                ];
                let at = match orientation {
                    1 => ((W - 1 - x) * H + y) * 3,
                    3 => (x * H + H - 1 - y) * 3,
                    _ => (y * W + x) * 3,
                };
                for c in 0..3 {
                    values[at + c] = i64::from(rgb[c].clamp(0, 255));
                }
            }
        }
    }
    let (w, h) = if matches!(orientation, 1 | 3) {
        (H, W)
    } else {
        (W, H)
    };
    Ok(array("RGB", w, h, 3, values))
}
