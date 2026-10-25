//! Decoder-only pinned AVIF/OpenJPEG boundary. No interpreter or dynamic codec lookup.
#![cfg(feature = "native-sdk")]
use std::{
    ffi::{c_char, CStr},
    fmt,
};
#[repr(C)]
#[derive(Default)]
struct NativePixels {
    width: u32,
    height: u32,
    components: u32,
    depth: u32,
    color_space: i32,
    pixels: *mut u8,
    length: usize,
}
extern "C" {
    fn image_codec_avif(
        bytes: *const u8,
        length: usize,
        max_pixels: u64,
        output: *mut NativePixels,
        message: *mut c_char,
        message_size: usize,
    ) -> i32;
    fn image_codec_jpeg2000(
        bytes: *const u8,
        length: usize,
        max_pixels: u64,
        output_depth: u32,
        preserve_palette: i32,
        output: *mut NativePixels,
        message: *mut c_char,
        message_size: usize,
    ) -> i32;
    fn image_codec_pixels_free(image: *mut NativePixels);
}
impl Drop for NativePixels {
    fn drop(&mut self) {
        unsafe { image_codec_pixels_free(self) }
    }
}
#[derive(Debug)]
pub struct DecodeError(String);
impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for DecodeError {}
#[derive(Debug)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub components: u32,
    pub depth: u32,
    pub color_space: i32,
    pub values: Vec<i64>,
}
fn convert(native: NativePixels) -> Result<Pixels, DecodeError> {
    let count = u64::from(native.width)
        .checked_mul(u64::from(native.height))
        .and_then(|n| n.checked_mul(u64::from(native.components)))
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| DecodeError("native codec pixel count overflow".into()))?;
    if ![8, 16].contains(&native.depth)
        || count.checked_mul(native.depth as usize / 8) != Some(native.length)
        || native.pixels.is_null()
    {
        return Err(DecodeError("invalid native image layout".into()));
    }
    let bytes = unsafe { std::slice::from_raw_parts(native.pixels, native.length) };
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|e| DecodeError(e.to_string()))?;
    if native.depth == 8 {
        values.extend(bytes.iter().map(|v| i64::from(*v)))
    } else {
        values.extend(
            bytes
                .chunks_exact(2)
                .map(|v| i64::from(u16::from_le_bytes([v[0], v[1]]))),
        )
    }
    Ok(Pixels {
        width: native.width,
        height: native.height,
        components: native.components,
        depth: native.depth,
        color_space: native.color_space,
        values,
    })
}
fn call(
    bytes: &[u8],
    max_pixels: u64,
    jpeg2000: Option<(u32, bool)>,
) -> Result<Pixels, DecodeError> {
    let mut native = NativePixels::default();
    let mut error = [0 as c_char; 512];
    let success = unsafe {
        if let Some((depth, palette)) = jpeg2000 {
            image_codec_jpeg2000(
                bytes.as_ptr(),
                bytes.len(),
                max_pixels,
                depth,
                i32::from(palette),
                &mut native,
                error.as_mut_ptr(),
                error.len(),
            )
        } else {
            image_codec_avif(
                bytes.as_ptr(),
                bytes.len(),
                max_pixels,
                &mut native,
                error.as_mut_ptr(),
                error.len(),
            )
        }
    };
    if success == 0 {
        return Err(DecodeError(
            unsafe { CStr::from_ptr(error.as_ptr()) }
                .to_string_lossy()
                .into_owned(),
        ));
    }
    convert(native)
}
pub fn avif(bytes: &[u8], max_pixels: u64) -> Result<Pixels, DecodeError> {
    call(bytes, max_pixels, None)
}
/// Rust callers select the original loaded mode's precision and palette policy.
/// Mode policy and SYCC/palette behavior are checked against independent original fixtures.
pub fn jpeg2000(
    bytes: &[u8],
    max_pixels: u64,
    depth: u32,
    preserve_palette: bool,
) -> Result<Pixels, DecodeError> {
    call(bytes, max_pixels, Some((depth, preserve_palette)))
}

pub mod jpeg2000_header;
pub mod policy;

mod ycbcr;

/// Packed TIFF codecs retain original array modes and sample interpretation.
pub mod tiff;
