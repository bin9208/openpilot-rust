//! Pillow/NumPy compatible image arrays for the native reference checker.
use std::{io::Cursor, path::Path};
#[path = "image_array/extended.rs"]
mod extended;
#[path = "image_array/legacy.rs"]
mod legacy;
#[path = "image_array/simple.rs"]
mod simple;
use crate::seal::Result;
#[derive(Debug)]
pub struct ImageArray {
    pub mode: String,
    pub shape: Vec<usize>,
    pub values: Vec<i64>,
}
type Array = ImageArray;
fn array(mode: &str, w: usize, h: usize, channels: usize, values: Vec<i64>) -> Array {
    let mut shape = vec![h, w];
    if channels > 1 {
        shape.push(channels)
    }
    Array {
        mode: mode.into(),
        shape,
        values,
    }
}
fn numpy_float(v: f64) -> i64 {
    if !v.is_finite() || !(-9223372036854775808.0..9223372036854775808.0).contains(&v) {
        i64::MIN
    } else {
        v.trunc() as i64
    }
}
fn png_decode(bytes: &[u8]) -> Result<Array> {
    let mut dec =
        png::Decoder::new_with_limits(Cursor::new(bytes), png::Limits { bytes: usize::MAX });
    dec.set_transformations(png::Transformations::IDENTITY);
    let mut reader = dec.read_info()?;
    check_dimensions(reader.info().width as usize, reader.info().height as usize)?;
    let mut buffer = zeroed(reader.output_buffer_size())?;
    let info = reader.next_frame(&mut buffer)?;
    let w = info.width as usize;
    let h = info.height as usize;
    let (mode, ch) = match info.color_type {
        png::ColorType::Indexed => ("P", 1),
        png::ColorType::Grayscale => (
            if info.bit_depth == png::BitDepth::One {
                "1"
            } else if info.bit_depth == png::BitDepth::Sixteen {
                "I;16"
            } else {
                "L"
            },
            1,
        ),
        png::ColorType::GrayscaleAlpha => ("LA", 2),
        png::ColorType::Rgb => ("RGB", 3),
        png::ColorType::Rgba => ("RGBA", 4),
    };
    let depth = info.bit_depth as usize;
    let mut values = vec![];
    values.try_reserve_exact(w * h * ch)?;
    for row in buffer[..info.buffer_size()].chunks(info.line_size) {
        if depth == 16 {
            for v in row.chunks_exact(2) {
                values.push(i64::from(u16::from_be_bytes([v[0], v[1]])));
            }
        } else if depth == 8 {
            values.extend(row.iter().map(|v| i64::from(*v)));
        } else {
            for x in 0..w * ch {
                let n = (row[x * depth / 8] >> (8 - depth - (x * depth % 8))) & ((1 << depth) - 1);
                values.push(if mode == "L" {
                    i64::from(n) * 255 / ((1 << depth) - 1)
                } else {
                    i64::from(n)
                });
            }
        }
    }
    // Pillow keeps grayscale16 values but truncates multi-channel16 to high bytes.
    if depth == 16 && ch > 1 {
        for v in &mut values {
            *v >>= 8;
        }
    }
    Ok(array(mode, w, h, ch, values))
}
fn gif_decode(bytes: &[u8]) -> Result<Array> {
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::Indexed);
    opts.set_memory_limit(gif::MemoryLimit::Unlimited);
    let mut dec = opts.read_info(Cursor::new(bytes))?;
    let w = dec.width() as usize;
    let h = dec.height() as usize;
    check_dimensions(w, h)?;
    let global = dec.global_palette().map(|p| p.to_vec());
    let background = dec.bg_color().unwrap_or(0) as i64;
    let frame = dec.read_next_frame()?.ok_or("no GIF frame")?;
    let mut vals = zeroed(w * h)?;
    vals.fill(background);
    for y in 0..frame.height as usize {
        for x in 0..frame.width as usize {
            let at = (y + frame.top as usize) * w + x + frame.left as usize;
            if at < vals.len() {
                vals[at] = frame.buffer[y * frame.width as usize + x] as i64;
            }
        }
    }
    let palette = frame.palette.as_deref().or(global.as_deref());
    let needed = palette.is_some_and(|p| {
        p.chunks_exact(3)
            .enumerate()
            .any(|(i, v)| v != [i as u8, i as u8, i as u8])
    });
    Ok(array(if needed { "P" } else { "L" }, w, h, 1, vals))
}
fn tiff_decode(bytes: &[u8]) -> Result<Array> {
    use tiff::decoder::{Decoder, DecodingResult};
    use tiff::tags::Tag;
    // NumPy receives palette indices. tiff's palette conversion is deliberately
    // bypassed by changing only the photometric tag in a private copy.
    let be = bytes.starts_with(b"MM");
    let u16_at = |at: usize| -> Result<u16> {
        let raw = bytes
            .get(at..at.checked_add(2).ok_or("TIFF offset overflow")?)
            .ok_or("TIFF entry truncated")?
            .try_into()?;
        Ok(if be {
            u16::from_be_bytes(raw)
        } else {
            u16::from_le_bytes(raw)
        })
    };
    let u32_at = |at: usize| -> Result<u32> {
        let raw = bytes
            .get(at..at.checked_add(4).ok_or("TIFF offset overflow")?)
            .ok_or("TIFF entry truncated")?
            .try_into()?;
        Ok(if be {
            u32::from_be_bytes(raw)
        } else {
            u32::from_le_bytes(raw)
        })
    };
    let directory = u32_at(4)? as usize;
    let count = u16_at(directory)? as usize;
    let mut patched = Vec::new();
    let mut palette = false;
    for i in 0..count {
        let at = directory
            .checked_add(2 + i * 12)
            .ok_or("TIFF directory overflow")?;
        if u16_at(at)? == 262
            && u16_at(at + 2)? == 3
            && u32_at(at + 4)? == 1
            && u16_at(at + 8)? == 3
        {
            patched.try_reserve_exact(bytes.len())?;
            patched.extend_from_slice(bytes);
            patched[at + 8..at + 10].copy_from_slice(&if be {
                1u16.to_be_bytes()
            } else {
                1u16.to_le_bytes()
            });
            palette = true;
            break;
        }
    }
    let input = if palette { patched.as_slice() } else { bytes };
    let mut dec = Decoder::new(Cursor::new(input))?.with_limits(tiff::decoder::Limits::unlimited());
    let (w, h) = dec.dimensions()?;
    check_dimensions(w as usize, h as usize)?;
    let color = dec.colortype()?;
    let sample = dec
        .get_tag_u32_vec(Tag::SampleFormat)
        .unwrap_or_else(|_| vec![1])[0];
    let (mut mode, ch, depth) = match color {
        tiff::ColorType::Gray(bits) => (
            if bits == 1 {
                "1"
            } else if bits == 16 && sample == 1 {
                if be {
                    "I;16B"
                } else {
                    "I;16"
                }
            } else if bits >= 16 {
                "I"
            } else {
                "L"
            },
            1,
            bits,
        ),
        tiff::ColorType::RGB(bits) => ("RGB", 3, bits),
        tiff::ColorType::RGBA(bits) => ("RGBA", 4, bits),
        tiff::ColorType::GrayA(bits) => ("LA", 2, bits),
        tiff::ColorType::Multiband {
            bit_depth: 8,
            num_samples: 2,
        } => ("LA", 2, 8),
        tiff::ColorType::CMYK(bits) => ("CMYK", 4, bits),
        _ => return Err(format!("unsupported TIFF {color:?}").into()),
    };
    if sample == 3 && depth != 32 || sample == 1 && depth == 32 && be {
        return Err("TIFF sample mode is not supported by Pillow".into());
    }
    if palette {
        mode = "P";
    }
    let planar = dec.get_tag_u32(Tag::PlanarConfiguration).unwrap_or(1) == 2;
    let mut data = DecodingResult::U8(Vec::new());
    dec.read_image_to_buffer(&mut data)?;
    let mut values: Vec<i64> = match data {
        DecodingResult::U8(v) => v.into_iter().map(i64::from).collect(),
        DecodingResult::I8(v) => v.into_iter().map(|v| i64::from(v as u8)).collect(),
        DecodingResult::U16(v) => v.into_iter().map(i64::from).collect(),
        DecodingResult::I16(v) => v.into_iter().map(i64::from).collect(),
        DecodingResult::U32(v) => v.into_iter().map(|v| i64::from(v as i32)).collect(),
        DecodingResult::I32(v) => v.into_iter().map(i64::from).collect(),
        DecodingResult::F32(v) => {
            mode = "F";
            v.into_iter().map(|v| numpy_float(f64::from(v))).collect()
        }
        _ => return Err("TIFF sample mode is not supported by Pillow".into()),
    };
    if depth < 8 {
        let packed = values;
        values = Vec::new();
        values.try_reserve_exact(w as usize * h as usize)?;
        let width = w as usize;
        let depth = depth as usize;
        let mask = (1 << depth) - 1;
        for row in packed.chunks((width * depth).div_ceil(8)) {
            for x in 0..width {
                let value = (row[x * depth / 8] >> (8 - depth - (x * depth % 8))) & mask;
                values.push(if mode == "L" {
                    value * 255 / mask
                } else {
                    value
                });
            }
        }
    }
    if depth == 16 && ch > 1 {
        for value in &mut values {
            *value >>= 8;
        }
    }
    if planar && ch > 1 {
        let plane = w as usize * h as usize;
        let mut interleaved = zeroed(values.len())?;
        if values.len() != plane * ch {
            return Err("TIFF incomplete planes".into());
        }
        for p in 0..plane {
            for c in 0..ch {
                interleaved[p * ch + c] = values[c * plane + p];
            }
        }
        values = interleaved;
    }
    Ok(array(mode, w as usize, h as usize, ch, values))
}
/// Read content by signature, independent of its filename extension.
pub fn read(path: &Path) -> Result<ImageArray> {
    decode(&std::fs::read(path)?)
}

pub const MAX_IMAGE_PIXELS: usize = 89_478_485;
/// Pillow 12.3's decompression-bomb policy, checked before pixel allocation.
pub fn pixel_warning(width: usize, height: usize) -> Result<Option<String>> {
    if width == 0 || height == 0 {
        return Err("image dimensions must be nonzero".into());
    }
    let pixels = width
        .checked_mul(height)
        .ok_or("image dimensions overflow")?;
    if pixels > MAX_IMAGE_PIXELS * 2 {
        return Err(format!("Image size ({pixels} pixels) exceeds limit of {} pixels, could be decompression bomb DOS attack.", MAX_IMAGE_PIXELS * 2).into());
    }
    Ok((pixels > MAX_IMAGE_PIXELS).then(|| format!("Image size ({pixels} pixels) exceeds limit of {MAX_IMAGE_PIXELS} pixels, could be decompression bomb DOS attack.")))
}
fn check_dimensions(width: usize, height: usize) -> Result<()> {
    if let Some(warning) = pixel_warning(width, height)? {
        eprintln!("DecompressionBombWarning: {warning}");
    }
    Ok(())
}
fn zeroed<T: Clone + Default>(count: usize) -> Result<Vec<T>> {
    let mut result = Vec::new();
    result.try_reserve_exact(count)?;
    result.resize(count, T::default());
    Ok(result)
}
/// Preserve the original Pillow mode and NumPy row-major signed integer samples.
pub fn decode(bytes: &[u8]) -> Result<ImageArray> {
    let result = decode_inner(bytes)?;
    let count = result.shape.iter().try_fold(1usize, |n, d| {
        n.checked_mul(*d).ok_or("image shape overflow")
    })?;
    if count != result.values.len() {
        return Err("decoded image samples do not match dimensions".into());
    }
    Ok(result)
}
fn decode_inner(bytes: &[u8]) -> Result<Array> {
    #[cfg(feature = "image-array-native")]
    {
        let avif = bytes.get(4..8) == Some(b"ftyp")
            && matches!(
                bytes.get(8..12),
                Some(b"avif" | b"avis" | b"mif1" | b"msf1")
            );
        let jpeg2000 = bytes.starts_with(&[255, 79, 255, 81])
            || bytes.starts_with(b"\0\0\0\x0cjP  \r\n\x87\n");
        let tiff = matches!(
            bytes.get(..4),
            Some(b"II\x2a\0" | b"MM\0\x2a" | b"II\x2b\0" | b"MM\0\x2b")
        );
        if tiff {
            let image = openpilot_image_codecs::tiff::decode(bytes, (MAX_IMAGE_PIXELS * 2) as u64)?;
            check_dimensions(image.width as usize, image.height as usize)?;
            return Ok(array(
                image.mode,
                image.width as usize,
                image.height as usize,
                image.channels,
                image.values,
            ));
        }
        if avif || jpeg2000 {
            let limit = (MAX_IMAGE_PIXELS * 2) as u64;
            let image = if avif {
                openpilot_image_codecs::policy::decode_avif(bytes, limit)?
            } else {
                openpilot_image_codecs::policy::decode_jpeg2000(bytes, limit)?
            };
            check_dimensions(image.width as usize, image.height as usize)?;
            return Ok(array(
                image.mode,
                image.width as usize,
                image.height as usize,
                image.components as usize,
                image.values,
            ));
        }
    }
    if let Some(image) = extended::decode(bytes)? {
        return Ok(image);
    }
    if let Some(image) = simple::decode(bytes)? {
        return Ok(image);
    }
    if bytes.starts_with(&[255, 216]) {
        let image = openpilot_jpeg::decode(bytes, (MAX_IMAGE_PIXELS * 2) as u64)?;
        check_dimensions(image.width as usize, image.height as usize)?;
        let mode = match image.components {
            1 => "L",
            3 => "RGB",
            4 => "CMYK",
            _ => return Err("invalid native JPEG components".into()),
        };
        let mut values = Vec::new();
        values.try_reserve_exact(image.pixels.len())?;
        values.extend(image.pixels.into_iter().map(i64::from));
        return Ok(array(
            mode,
            image.width as usize,
            image.height as usize,
            image.components as usize,
            values,
        ));
    }

    if bytes.starts_with(b"icns") {
        return legacy::icns(bytes);
    }
    if bytes.starts_with(b"\x01\xda") {
        return legacy::sgi(bytes);
    }
    if bytes.starts_with(b"DDS ") {
        if let Some(v) = legacy::dds(bytes)? {
            return Ok(v);
        }
    }
    if bytes.starts_with(b"BM") {
        if let Some(v) = legacy::bmp(bytes)? {
            return Ok(v);
        }
    }
    if bytes.starts_with(b"\x0a") {
        return legacy::pcx(bytes);
    }
    if bytes.starts_with(b"P4") {
        return legacy::pbm(bytes);
    }
    if bytes.starts_with(b"Pf") || bytes.starts_with(b"PF") {
        return legacy::pfm(bytes);
    }
    if bytes.get(..100).unwrap_or(bytes).contains(&b'\n') {
        if let Some(v) = legacy::im(bytes)? {
            return Ok(v);
        }
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return png_decode(bytes);
    }
    if bytes.starts_with(b"GIF8") {
        return gif_decode(bytes);
    }
    if bytes.starts_with(b"II\x2a\0") || bytes.starts_with(b"MM\0\x2a") {
        return tiff_decode(bytes);
    }
    let format = image::guess_format(bytes).or_else(|_| {
        if bytes.len() > 18 {
            Ok(image::ImageFormat::Tga)
        } else {
            Err(image::ImageError::IoError(
                std::io::ErrorKind::InvalidData.into(),
            ))
        }
    })?;
    if format == image::ImageFormat::Tga {
        if let Some(v) = legacy::tga(bytes)? {
            return Ok(v);
        }
    }
    let dimensions =
        image::ImageReader::with_format(Cursor::new(bytes), format).into_dimensions()?;
    check_dimensions(dimensions.0 as usize, dimensions.1 as usize)?;
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    reader.no_limits();
    let img = reader.decode()?;
    let w = img.width() as usize;
    let h = img.height() as usize;
    use image::DynamicImage::*;
    Ok(match img {
        ImageLuma8(v) => array(
            "L",
            w,
            h,
            1,
            v.into_raw().into_iter().map(i64::from).collect(),
        ),
        ImageLuma16(v) => array(
            "I",
            w,
            h,
            1,
            v.into_raw().into_iter().map(i64::from).collect(),
        ),
        ImageLumaA8(v) => array(
            "LA",
            w,
            h,
            2,
            v.into_raw().into_iter().map(i64::from).collect(),
        ),
        ImageRgb8(v) => array(
            "RGB",
            w,
            h,
            3,
            v.into_raw().into_iter().map(i64::from).collect(),
        ),
        ImageRgba8(v) => array(
            "RGBA",
            w,
            h,
            4,
            v.into_raw().into_iter().map(i64::from).collect(),
        ),
        _ => return Err("generic high-depth pending".into()),
    })
}
