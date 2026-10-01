use crate::Error;
use num_traits::ToPrimitive;
use rustybuzz::{
    ttf_parser::{GlyphId, RasterImageFormat},
    Face, UnicodeBuffer,
};
use std::io::Cursor;

pub fn is_emoji(c: char) -> bool {
    matches!(c, '\u{1f600}'..='\u{1f64f}' | '\u{1f300}'..='\u{1f5ff}' | '\u{1f680}'..='\u{1f6ff}' | '\u{1f1e0}'..='\u{1f1ff}' | '\u{1f900}'..='\u{1f9ff}' | '\u{2300}'..='\u{23ff}' | '\u{2600}'..='\u{2bff}' | '\u{1fa70}'..='\u{1faff}' | '\u{1f700}'..='\u{1f77f}' | '\u{200d}' | '\u{fe0f}' | '\u{3030}')
}
pub fn find(text: &str) -> Vec<(usize, usize)> {
    let mut result = Vec::new();
    let mut start = None;
    for (index, c) in text.char_indices() {
        if is_emoji(c) {
            if start.is_none() {
                start = Some(index);
            }
        } else if let Some(start) = start.take() {
            result.push((start, index));
        }
    }
    if let Some(start) = start {
        result.push((start, text.len()));
    }
    result
}

/// The source draws shaped Noto bitmap glyphs into a fixed 128-square transparent canvas.
pub fn rasterize(font: &[u8], text: &str) -> Result<Vec<u8>, Error> {
    let mut face = Face::from_slice(font, 0).ok_or(Error::Contract("invalid emoji font"))?;
    face.set_pixels_per_em(Some((109, 109)));
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    let glyphs = rustybuzz::shape(&face, &[], buffer);
    let scale = 109.0 / f64::from(face.units_per_em());
    let ascender = (f64::from(face.ascender()) * scale).round();
    let mut cursor = 0.0;
    let mut canvas = vec![0; 128 * 128 * 4];
    for (info, position) in glyphs.glyph_infos().iter().zip(glyphs.glyph_positions()) {
        let id = GlyphId(
            u16::try_from(info.glyph_id)
                .map_err(|_| Error::Contract("emoji glyph index overflow"))?,
        );
        if let Some(image) = face.glyph_raster_image(id, 109) {
            if image.format != RasterImageFormat::PNG {
                return Err(Error::Contract("emoji glyph is not PNG"));
            }
            let mut decoder = png::Decoder::new(Cursor::new(image.data));
            decoder
                .set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
            let mut reader = decoder
                .read_info()
                .map_err(|_| Error::Contract("emoji PNG header invalid"))?;
            let mut pixels = vec![0; reader.output_buffer_size()];
            let output = reader
                .next_frame(&mut pixels)
                .map_err(|_| Error::Contract("emoji PNG payload invalid"))?;
            if output.color_type != png::ColorType::Rgba {
                return Err(Error::Contract("emoji PNG is not RGBA"));
            }
            let x = (cursor + f64::from(position.x_offset) * scale)
                .round()
                .to_i32()
                .ok_or(Error::Contract("emoji x overflow"))?
                + i32::from(image.x);
            let y = (ascender
                - f64::from(position.y_offset) * scale
                - f64::from(image.y)
                - f64::from(image.height))
            .round()
            .to_i32()
            .ok_or(Error::Contract("emoji y overflow"))?;
            for row in 0..output.height {
                for column in 0..output.width {
                    let target_x = x + i32::try_from(column)
                        .map_err(|_| Error::Contract("emoji width overflow"))?;
                    let target_y = y + i32::try_from(row)
                        .map_err(|_| Error::Contract("emoji height overflow"))?;
                    if !(0..128).contains(&target_x) || !(0..128).contains(&target_y) {
                        continue;
                    }
                    let from = usize::try_from(
                        (u64::from(row) * u64::from(output.width) + u64::from(column)) * 4,
                    )
                    .map_err(|_| Error::Contract("emoji index overflow"))?;
                    let to = usize::try_from((target_y * 128 + target_x) * 4)
                        .map_err(|_| Error::Contract("emoji index overflow"))?;
                    let alpha = u32::from(pixels[from + 3]);
                    if alpha == 0 {
                        continue;
                    }
                    let old_alpha = u32::from(canvas[to + 3]);
                    let combined = alpha * 255 + old_alpha * (255 - alpha);
                    for channel in 0..3 {
                        let premultiplied = (u32::from(pixels[from + channel]) * alpha + 127) / 255;
                        let straight = (255 * premultiplied / alpha).min(255);
                        let value = if old_alpha > 0 {
                            (straight * alpha
                                + u32::from(canvas[to + channel]) * (255 - alpha)
                                + 127)
                                / 255
                        } else {
                            straight
                        };
                        canvas[to + channel] = u8::try_from(value)
                            .map_err(|_| Error::Contract("emoji blend overflow"))?;
                    }
                    canvas[to + 3] = u8::try_from((combined + 127) / 255)
                        .map_err(|_| Error::Contract("emoji alpha overflow"))?;
                }
            }
        }
        cursor += f64::from(position.x_advance) * scale;
    }
    // FreeType's premultiplied glyph is unpremultiplied by Pillow before its
    // alpha-mask paste; preserve both integer rounding stages on a blank canvas.
    for pixel in canvas.chunks_exact_mut(4) {
        let alpha = u32::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = u8::try_from((u32::from(*channel) * alpha + 127) / 255)
                .map_err(|_| Error::Contract("emoji paste overflow"))?;
        }
    }
    Ok(canvas)
}
#[derive(Clone, Debug)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub text: String,
}
pub fn spans(text: &str) -> Vec<Span> {
    find(text)
        .into_iter()
        .map(|(start, end)| Span {
            start: text[..start].chars().count(),
            end: text[..end].chars().count(),
            text: text[start..end].to_owned(),
        })
        .collect()
}
