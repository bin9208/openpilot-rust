//! Independent fixed-canvas emoji policy from pinned emoji.py and Pillow 12.3.0.
//! Shaping and bitmap tables come from the font; candidate UI code is not used.
use crate::seal::Result;
use rustybuzz::{
    ttf_parser::{GlyphId, RasterImageFormat},
    Face, UnicodeBuffer,
};
use std::io::Cursor;
const SIDE: usize = 128;
const STRIKE: u16 = 109;
fn mul255(a: u32, b: u32) -> u32 {
    let n = a * b + 128;
    (n + (n >> 8)) >> 8
}
fn blend(alpha: u32, destination: u32, source: u32) -> u32 {
    let n = destination * (255 - alpha) + source * alpha + 128;
    (n + (n >> 8)) >> 8
}
fn pixel(v: i64) -> i64 {
    (v + 32) >> 6
}
/// Draw at Pillow's default left/ascender anchor into transparent RGBA128x128.
pub fn raster(font: &[u8], text: &str) -> Result<Vec<u8>> {
    if text.chars().count() > 1_000_000 {
        return Err("original Pillow text length limit".into());
    }
    let mut face = Face::from_slice(font, 0).ok_or("invalid Forms emoji font")?;
    face.set_pixels_per_em(Some((STRIKE, STRIKE)));
    let mut input = UnicodeBuffer::new();
    input.push_str(text);
    let shaped = rustybuzz::shape(&face, &[], input);
    let factor = f64::from(STRIKE) / f64::from(face.units_per_em());
    let ascender = (f64::from(face.ascender()) * factor).round() as i64;
    let to_fixed = |v: i32| (f64::from(v) * factor * 64.0).round() as i64;
    let mut pen = (0i64, 0i64);
    let mut canvas = vec![0u8; SIDE * SIDE * 4];
    for (glyph, offset) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
        let id = GlyphId(u16::try_from(glyph.glyph_id)?);
        if let Some(bitmap) = face.glyph_raster_image(id, STRIKE) {
            if bitmap.format != RasterImageFormat::PNG {
                return Err("Forms pinned emoji bitmap format changed".into());
            }
            let mut decoder = png::Decoder::new(Cursor::new(bitmap.data));
            decoder
                .set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
            let mut decoder = decoder.read_info()?;
            let mut bytes = vec![0; decoder.output_buffer_size()];
            let info = decoder.next_frame(&mut bytes)?;
            if info.color_type != png::ColorType::Rgba
                || info.width != u32::from(bitmap.width)
                || info.height != u32::from(bitmap.height)
            {
                return Err("Forms pinned emoji RGBA dimensions changed".into());
            }
            let left = pixel(pen.0 + to_fixed(offset.x_offset)) + i64::from(bitmap.x);
            let top = ascender
                - pixel(pen.1 + to_fixed(offset.y_offset))
                - i64::from(bitmap.y)
                - i64::from(bitmap.height);
            for y in 0..usize::try_from(info.height)? {
                for x in 0..usize::try_from(info.width)? {
                    let (dx, dy) = (left + x as i64, top + y as i64);
                    if !(0..SIDE as i64).contains(&dx) || !(0..SIDE as i64).contains(&dy) {
                        continue;
                    }
                    let source = &bytes[(y * info.width as usize + x) * 4..][..4];
                    let a = u32::from(source[3]);
                    if a == 0 {
                        continue;
                    }
                    let target = &mut canvas[(dy as usize * SIDE + dx as usize) * 4..][..4];
                    let occupied = target[3] != 0;
                    for channel in 0..3 {
                        // FreeType decodes PNG to premultiplied BGRA; Pillow's
                        // imagingft unpremultiplies by truncating division.
                        let premul = mul255(u32::from(source[channel]), a);
                        let straight = (255 * premul / a).min(255);
                        target[channel] = if occupied {
                            blend(a, u32::from(target[channel]), straight)
                        } else {
                            straight
                        }
                        .min(255) as u8;
                    }
                    target[3] = (a + mul255(u32::from(target[3]), 255 - a)).min(255) as u8;
                }
            }
        }
        pen.0 += to_fixed(offset.x_advance);
        pen.1 += to_fixed(offset.y_advance);
    }
    // ImageDraw extracts the glyph alpha mask, sets RGB image alpha to255,
    // then pastes through that mask onto the transparent128-square canvas.
    for rgba in canvas.chunks_exact_mut(4) {
        let alpha = u32::from(rgba[3]);
        for channel in &mut rgba[..3] {
            *channel = mul255(u32::from(*channel), alpha) as u8;
        }
    }
    Ok(canvas)
}
