use crate::{
    draw::{Draw, BLACK},
    geometry::Point,
    text_layout::{self as text, float, TextStyle},
    Error,
};
#[derive(Clone, Copy, Debug, Default)]
pub enum Anchor {
    #[default]
    CenterBottom,
    CenterTop,
    LeftTop,
    RightTop,
    LeftCenter,
    Center,
    RightCenter,
}
pub struct OutlineStyle {
    pub text: TextStyle,
    pub border_width: f64,
    pub shadow_offset: f64,
    pub border_color: u32,
    pub shadow_color: u32,
    pub anchor: Anchor,
    pub y_offset: f64,
}
impl OutlineStyle {
    pub fn new(text: TextStyle) -> Self {
        Self {
            text,
            border_width: 3.0,
            shadow_offset: 8.0,
            border_color: BLACK,
            shadow_color: BLACK,
            anchor: Anchor::CenterBottom,
            y_offset: 6.0,
        }
    }
}
pub fn position(
    draw: &dyn Draw,
    value: &str,
    point: Point,
    style: &OutlineStyle,
) -> (Point, Point) {
    let measured = text::measure(draw, style.text.font, value, style.text.size, 0.0);
    let x = f64::from(point.x)
        - match style.anchor {
            Anchor::CenterBottom | Anchor::CenterTop | Anchor::Center => {
                f64::from(measured.x) * 0.5
            }
            Anchor::RightTop | Anchor::RightCenter => f64::from(measured.x),
            _ => 0.0,
        };
    let y = f64::from(point.y) + style.y_offset
        - match style.anchor {
            Anchor::CenterBottom => f64::from(measured.y),
            Anchor::LeftCenter | Anchor::Center | Anchor::RightCenter => {
                f64::from(measured.y) * 0.5
            }
            _ => 0.0,
        };
    (
        Point {
            x: float(x),
            y: float(y),
        },
        measured,
    )
}
pub fn draw(
    draw: &mut dyn Draw,
    value: &str,
    point: Point,
    style: &OutlineStyle,
) -> Result<(), Error> {
    if value.is_empty() {
        return Ok(());
    }
    let (position, _) = position(draw, value, point, style);
    if style.border_width > 0.0 {
        for step in 0..8 {
            let angle = f64::from(step * 45).to_radians();
            text::draw_text(
                draw,
                style.text.font,
                value,
                Point {
                    x: float(f64::from(position.x) + style.border_width * angle.cos()),
                    y: float(f64::from(position.y) + style.border_width * angle.sin()),
                },
                style.text.size,
                0.0,
                style.border_color,
            )?;
        }
    }
    if style.shadow_offset != 0.0 {
        text::draw_text(
            draw,
            style.text.font,
            value,
            Point {
                x: float(f64::from(position.x) + style.shadow_offset),
                y: float(f64::from(position.y) + style.shadow_offset),
            },
            style.text.size,
            0.0,
            style.shadow_color,
        )?;
    }
    text::draw_text(
        draw,
        style.text.font,
        value,
        position,
        style.text.size,
        0.0,
        style.text.color,
    )
}
