//! Original TIFF mode/array policy around a bounded pinned libtiff decoder.
use crate::DecodeError;
#[path = "tiff_header.rs"]
mod header;
#[path = "tiff_modes.rs"]
mod modes;
use std::ffi::{c_char, CStr};
#[repr(C)]
#[derive(Default)]
struct Native {
    width: u32,
    height: u32,
    bits: u16,
    channels: u16,
    sample_format: u16,
    photometric: u16,
    compression: u16,
    orientation: u16,
    fillorder: u16,
    planar: u16,
    extra_count: u16,
    extras: [u16; 6],
    rgba_conversion: u16,
    pixels: *mut u8,
    length: usize,
    stride: usize,
}
extern "C" {
    fn image_codec_tiff_decode(
        bytes: *const u8,
        length: usize,
        limit: u64,
        output: *mut Native,
        message: *mut c_char,
        message_size: usize,
    ) -> i32;
    fn image_codec_tiff_free(image: *mut Native);
}
impl Drop for Native {
    fn drop(&mut self) {
        unsafe { image_codec_tiff_free(self) }
    }
}
#[derive(Debug)]
pub struct Image {
    pub mode: &'static str,
    pub width: u32,
    pub height: u32,
    pub channels: usize,
    pub values: Vec<i64>,
}
fn error(s: &str) -> DecodeError {
    DecodeError(s.into())
}
fn float(v: f32) -> i64 {
    let v = f64::from(v);
    if !v.is_finite() || !(-9223372036854775808.0..9223372036854775808.0).contains(&v) {
        i64::MIN
    } else {
        v.trunc() as i64
    }
}
fn u16at(bytes: &[u8], at: usize, be: bool) -> Result<u16, DecodeError> {
    let raw = bytes
        .get(at..at + 2)
        .ok_or_else(|| error("TIFF header truncated"))?
        .try_into()
        .unwrap();
    Ok(if be {
        u16::from_be_bytes(raw)
    } else {
        u16::from_le_bytes(raw)
    })
}
fn allocate(n: usize) -> Result<Vec<i64>, DecodeError> {
    let mut v = Vec::new();
    v.try_reserve_exact(n)
        .map_err(|_| error("TIFF array allocation failed"))?;
    v.resize(n, 0);
    Ok(v)
}
pub fn decode(bytes: &[u8], limit: u64) -> Result<Image, DecodeError> {
    let selected = header::parse(bytes)?;
    if selected.compression == 1 && matches!(selected.rawmode, "L;IR" | "P;1R" | "P;2R" | "P;4R") {
        return Err(error("TIFF rawmode has no original unpacker"));
    }
    let be = bytes.starts_with(b"MM");
    let mut native = Native::default();
    let mut message = [0 as c_char; 1024];
    if unsafe {
        image_codec_tiff_decode(
            bytes.as_ptr(),
            bytes.len(),
            limit,
            &mut native,
            message.as_mut_ptr(),
            message.len(),
        )
    } == 0
    {
        return Err(DecodeError(
            unsafe { CStr::from_ptr(message.as_ptr()) }
                .to_string_lossy()
                .into_owned(),
        ));
    }
    let (w, h, bits, ch) = (
        native.width as usize,
        native.height as usize,
        native.bits as usize,
        native.channels as usize,
    );
    if w == 0
        || h == 0
        || w.checked_mul(h).is_none_or(|v| v as u64 > limit)
        || native.pixels.is_null()
    {
        return Err(error("TIFF decoded dimensions invalid"));
    }
    // Match Pillow OPEN_INFO's mode admission, retaining palette indices,
    // signed integer interpretation and high-byte multichannel16 policy.
    let extras = selected.extras.as_slice();
    let (mode, rawmode) = (selected.mode, selected.rawmode);
    let channels = match mode {
        "RGB" | "LAB" => 3,
        "RGBA" | "CMYK" => 4,
        "LA" | "PA" => 2,
        _ => 1,
    };
    // Raw TIFF uses the file endian; libtiff always returns native endian.
    let source = unsafe { std::slice::from_raw_parts(native.pixels, native.length) };
    let mut values = allocate(w * h * channels)?;
    for y in 0..h {
        for x in 0..w {
            for c in 0..channels {
                let mut value = if native.rgba_conversion != 0 {
                    let at = (y * w + x) * 4;
                    let word = u32::from_ne_bytes(
                        source
                            .get(at..at + 4)
                            .ok_or_else(|| error("TIFF RGBA output truncated"))?
                            .try_into()
                            .unwrap(),
                    );
                    i64::from((word >> (c * 8)) & 255)
                } else {
                    let bit = (x * ch + c) * bits;
                    let at = y * native.stride + bit / 8;
                    let p = source
                        .get(at..at + bits.div_ceil(8))
                        .ok_or_else(|| error("TIFF sample output truncated"))?;
                    if bits == 12 {
                        let row = &source[y * native.stride..(y + 1) * native.stride];
                        let mut value = 0i64;
                        for i in 0..12 {
                            let at = bit + i;
                            value = (value << 1) | i64::from((row[at / 8] >> (7 - at % 8)) & 1);
                        }
                        value
                    } else if bits < 8 {
                        i64::from((p[0] >> (8 - bits - bit % 8)) & ((1u16 << bits) - 1) as u8)
                    } else if bits == 8 {
                        i64::from(p[0])
                    } else if bits == 16 {
                        let n = u16::from_ne_bytes(p.try_into().unwrap());
                        if native.sample_format == 2 {
                            i64::from(if native.compression != 1 && rawmode == "I;16BS" {
                                i16::from_be_bytes(p.try_into().unwrap())
                            } else {
                                n as i16
                            })
                        } else if channels > 1 {
                            i64::from(n >> 8)
                        } else {
                            i64::from(n)
                        }
                    } else if native.sample_format == 3 {
                        float(if native.compression != 1 && rawmode == "F;32BF" {
                            f32::from_be_bytes(p.try_into().unwrap())
                        } else {
                            f32::from_ne_bytes(p.try_into().unwrap())
                        })
                    } else {
                        i64::from(if native.compression != 1 && rawmode == "I;32BS" {
                            i32::from_be_bytes(p.try_into().unwrap())
                        } else {
                            i32::from_ne_bytes(p.try_into().unwrap())
                        })
                    }
                };
                if native.photometric == 0
                    && rawmode
                        .split_once(';')
                        .is_some_and(|(_, suffix)| suffix.contains('I'))
                {
                    value = if mode == "1" {
                        1 - value
                    } else if mode == "L" {
                        ((1i64 << bits.min(8)) - 1) - value
                    } else {
                        value
                    };
                }
                if bits < 8 && mode == "L" {
                    value = value * 255 / ((1 << bits) - 1);
                }

                values[(y * w + x) * channels + c] = value;
            }
            if mode == "RGBA" && extras.first() == Some(&1) {
                let at = (y * w + x) * 4;
                let alpha = values[at + 3];
                if alpha != 0 && alpha != 255 {
                    for c in 0..3 {
                        values[at + c] = (values[at + c] * 255 / alpha).min(255);
                    }
                }
            }
        }
    }
    let orientation = selected.orientation;
    if (2..=8).contains(&orientation) {
        let (ow, oh) = if orientation >= 5 { (h, w) } else { (w, h) };
        let mut rotated = allocate(values.len())?;
        for y in 0..h {
            for x in 0..w {
                let (dx, dy) = match orientation {
                    2 => (w - 1 - x, y),
                    3 => (w - 1 - x, h - 1 - y),
                    4 => (x, h - 1 - y),
                    5 => (y, x),
                    6 => (h - 1 - y, x),
                    7 => (h - 1 - y, w - 1 - x),
                    _ => (y, w - 1 - x),
                };
                rotated[(dy * ow + dx) * channels..(dy * ow + dx + 1) * channels]
                    .copy_from_slice(&values[(y * w + x) * channels..(y * w + x + 1) * channels]);
            }
        }
        return Ok(Image {
            mode,
            width: ow as u32,
            height: oh as u32,
            channels,
            values: rotated,
        });
    }
    // Reading the header here additionally rejects unexpected endian markers.
    if u16at(bytes, 2, be)? != 42 && u16at(bytes, 2, be)? != 43 {
        return Err(error("TIFF header version"));
    }
    Ok(Image {
        mode,
        width: native.width,
        height: native.height,
        channels,
        values,
    })
}
