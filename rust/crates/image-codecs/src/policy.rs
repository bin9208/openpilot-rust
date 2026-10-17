//! Public mode policy; native codec selection never normalizes to RGBA.
use crate::{avif, jpeg2000, jpeg2000_header, DecodeError, Pixels};
#[derive(Debug)]
pub struct Image {
    pub mode: &'static str,
    pub width: u32,
    pub height: u32,
    pub components: u32,
    pub values: Vec<i64>,
}
fn image(mode: &'static str, pixels: Pixels) -> Image {
    Image {
        mode,
        width: pixels.width,
        height: pixels.height,
        components: pixels.components,
        values: pixels.values,
    }
}
pub fn decode_avif(bytes: &[u8], limit: u64) -> Result<Image, DecodeError> {
    let pixels = avif(bytes, limit)?;
    let mode = match pixels.components {
        1 => "L",
        3 => "RGB",
        4 => "RGBA",
        _ => return Err(DecodeError("AVIF component count invalid".into())),
    };
    Ok(image(mode, pixels))
}
pub fn decode_jpeg2000(bytes: &[u8], limit: u64) -> Result<Image, DecodeError> {
    let header = jpeg2000_header::parse(bytes)?;
    if u64::from(header.width) * u64::from(header.height) > limit {
        return Err(DecodeError("JPEG2000 pixel limit exceeded".into()));
    }
    let mut pixels = jpeg2000(
        bytes,
        limit,
        if header.mode == "I;16" { 16 } else { 8 },
        header.palette,
    )?;
    if pixels.width != header.width || pixels.height != header.height {
        return Err(DecodeError(
            "JPEG2000 decoded dimensions differ from header".into(),
        ));
    }
    // Original loaded modes determine the channel layout, including palette indices.
    let channels = match header.mode {
        "LA" | "PA" => 2,
        "RGB" => 3,
        "RGBA" | "CMYK" => 4,
        _ => 1,
    };
    if channels != pixels.components {
        return Err(DecodeError(
            "JPEG2000 decoded component layout differs from original mode".into(),
        ));
    }
    if pixels.color_space == 3 && matches!(header.mode, "RGB" | "RGBA") {
        crate::ycbcr::convert(&mut pixels.values, pixels.components as usize);
    }
    Ok(image(header.mode, pixels))
}
