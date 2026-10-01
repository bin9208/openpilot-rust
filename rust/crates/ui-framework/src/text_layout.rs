use crate::{
    draw::{Draw, TextDraw},
    emoji,
    geometry::Point,
    text::{whitespace, Font},
    Error,
};
use num_traits::ToPrimitive;

pub fn float(value: f64) -> f32 {
    value.to_f32().unwrap_or(if value.is_sign_negative() {
        f32::NEG_INFINITY
    } else {
        f32::INFINITY
    })
}
pub fn measure(draw: &dyn Draw, font: Font, text: &str, size: f64, spacing: f64) -> Point {
    let (plain, count) = crate::text::strip_emoji(text);
    let scaled = float(size * draw.font_scale());
    let spacing = (spacing * 10000.0).round_ties_even() / 10000.0;
    let mut result = draw.measure(font, &plain, scaled, float(spacing));
    if count > 0 {
        result.x = float(f64::from(result.x) + f64::from(count) * size * draw.font_scale());
        if result.y == 0.0 {
            result.y = scaled;
        }
    }
    result
}
pub fn draw_text(
    draw: &mut dyn Draw,
    font: Font,
    text: &str,
    position: Point,
    size: f64,
    spacing: f64,
    color: u32,
) -> Result<(), Error> {
    let scaled = float(size * draw.font_scale());
    draw.text(TextDraw {
        font,
        text,
        position,
        size: scaled,
        spacing: float(spacing),
        color,
    })
}
pub fn draw_emoji_line(
    draw: &mut dyn Draw,
    font: Font,
    text: &str,
    mut position: Point,
    size: f64,
    spacing: f64,
    color: u32,
) -> Result<(), Error> {
    let mut previous = 0;
    for (start, end) in emoji::find(text) {
        let before = &text[previous..start];
        if !before.is_empty() {
            draw_text(draw, font, before, position, size, spacing, color)?;
            position.x += measure(draw, font, before, size, spacing).x;
        }
        let emoji_size = float(size * draw.font_scale());
        draw.emoji(&text[start..end], position, emoji_size, color)?;
        position.x += emoji_size;
        previous = end;
    }
    let after = &text[previous..];
    if !after.is_empty() {
        draw_text(draw, font, after, position, size, spacing, color)?;
    }
    Ok(())
}
pub fn elide(
    draw: &dyn Draw,
    font: Font,
    text: &str,
    size: f64,
    spacing: f64,
    width: f64,
    force: bool,
) -> String {
    let text_width = f64::from(measure(draw, font, text, size, spacing).x);
    if text_width <= width {
        if !force {
            return text.to_owned();
        }
        if text_width + f64::from(measure(draw, font, "...", size, spacing).x) <= width {
            return format!("{text}...");
        }
    }
    let characters: Vec<_> = text.chars().collect();
    let (mut left, mut right) = (0, characters.len());
    while left < right {
        let middle = (left + right) / 2;
        let candidate = format!("{}...", characters[..middle].iter().collect::<String>());
        if f64::from(measure(draw, font, &candidate, size, spacing).x) <= width {
            left = middle + 1;
        } else {
            right = middle;
        }
    }
    format!(
        "{}...",
        characters[..left.saturating_sub(1)]
            .iter()
            .collect::<String>()
    )
}
pub fn wrap(
    draw: &dyn Draw,
    font: Font,
    text: &str,
    size: f64,
    spacing: f64,
    width: f64,
) -> Vec<String> {
    if text.is_empty() || width <= 0.0 {
        return Vec::new();
    }
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut current = String::new();
        if paragraph.trim_matches(whitespace).is_empty() {
            lines.push(String::new());
            continue;
        }
        for word in paragraph.split(whitespace).filter(|word| !word.is_empty()) {
            if f64::from(measure(draw, font, word, size, spacing).x) > width {
                if !current.is_empty() {
                    lines.push(std::mem::take(&mut current));
                }
                let mut remaining = word;
                while !remaining.is_empty() {
                    if f64::from(measure(draw, font, remaining, size, spacing).x) <= width {
                        lines.push(remaining.to_owned());
                        break;
                    }
                    let chars: Vec<_> = remaining
                        .char_indices()
                        .map(|(index, _)| index)
                        .chain(std::iter::once(remaining.len()))
                        .collect();
                    let (mut left, mut right, mut best) = (1, chars.len() - 1, 1);
                    while left <= right {
                        let middle = (left + right) / 2;
                        if f64::from(
                            measure(draw, font, &remaining[..chars[middle]], size, spacing).x,
                        ) <= width
                        {
                            best = middle;
                            left = middle + 1;
                        } else {
                            right = middle - 1;
                        }
                    }
                    lines.push(remaining[..chars[best]].to_owned());
                    remaining = &remaining[chars[best]..];
                }
                continue;
            }
            let candidate = if current.is_empty() {
                word.to_owned()
            } else {
                format!("{current} {word}")
            };
            if f64::from(measure(draw, font, &candidate, size, spacing).x) <= width {
                current = candidate;
            } else {
                if !current.is_empty() {
                    lines.push(current);
                }
                current = word.to_owned();
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    lines
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Horizontal {
    #[default]
    Left,
    Center,
    Right,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Vertical {
    #[default]
    Top,
    Middle,
    Bottom,
}
#[derive(Clone, Copy, Debug)]
pub struct TextStyle {
    pub font: Font,
    pub size: f64,
    pub spacing: f64,
    pub color: u32,
}
pub fn draw_emoji_spans(
    draw: &mut dyn Draw,
    value: &str,
    spans: &[emoji::Span],
    mut position: Point,
    style: TextStyle,
) -> Result<(), Error> {
    let TextStyle {
        font,
        size,
        spacing,
        color,
    } = style;
    let chars: Vec<_> = value.chars().collect();
    let mut previous = 0;
    for span in spans {
        let before: String = chars[previous.min(chars.len())..span.start.min(chars.len())]
            .iter()
            .collect();
        if !before.is_empty() {
            draw_text(draw, font, &before, position, size, spacing, color)?;
            position.x += measure(draw, font, &before, size, spacing).x;
        }
        let scaled = float(size * draw.font_scale());
        draw.emoji(&span.text, position, scaled, color)?;
        position.x += scaled;
        previous = span.end;
    }
    let after: String = chars[previous.min(chars.len())..].iter().collect();
    if !after.is_empty() {
        draw_text(draw, font, &after, position, size, spacing, color)?;
    }
    Ok(())
}
