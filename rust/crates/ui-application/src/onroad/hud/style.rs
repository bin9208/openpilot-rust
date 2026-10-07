use crate::{onroad::model_renderer::math::float, paint::color};
use openpilot_ui_framework::{
    draw::{Draw, TextDraw},
    geometry::Point,
    text::Font,
    text_layout, Error,
};
#[derive(Clone, Copy)]
pub enum Anchor {
    CenterBottom,
    CenterTop,
    LeftTop,
    RightTop,
    LeftCenter,
    Center,
    RightCenter,
}
pub struct Text<'a> {
    pub value: &'a str,
    pub position: [f64; 2],
    pub size: f64,
    pub font: Font,
    pub color: u32,
    pub anchor: Anchor,
    pub border: f64,
    pub shadow: f64,
    pub y_offset: f64,
}
pub fn text(draw: &mut dyn Draw, label: Text<'_>) -> Result<(), Error> {
    if label.value.is_empty() {
        return Ok(());
    }
    let measured = text_layout::measure(draw, label.font, label.value, label.size, 0.0);
    let [mut x, mut y] = label.position;
    y += label.y_offset;
    match label.anchor {
        Anchor::CenterBottom => {
            x -= f64::from(measured.x) * 0.5;
            y -= f64::from(measured.y);
        }
        Anchor::CenterTop => {
            x -= f64::from(measured.x) * 0.5;
        }
        Anchor::LeftTop => {}
        Anchor::RightTop => {
            x -= f64::from(measured.x);
        }
        Anchor::LeftCenter => {
            y -= f64::from(measured.y) * 0.5;
        }
        Anchor::Center => {
            x -= f64::from(measured.x) * 0.5;
            y -= f64::from(measured.y) * 0.5;
        }
        Anchor::RightCenter => {
            x -= f64::from(measured.x);
            y -= f64::from(measured.y) * 0.5;
        }
    }
    let size = float(label.size * draw.font_scale());
    let mut put = |position, color| {
        draw.text(TextDraw {
            font: label.font,
            text: label.value,
            position,
            size,
            spacing: 0.0,
            color,
        })
    };
    if label.border > 0.0 {
        for step in 0..8 {
            let angle = f64::from(step * 45).to_radians();
            put(
                Point {
                    x: float(x + label.border * angle.cos()),
                    y: float(y + label.border * angle.sin()),
                },
                color(0, 0, 0, 255),
            )?;
        }
    }
    if label.shadow != 0.0 {
        put(
            Point {
                x: float(x + label.shadow),
                y: float(y + label.shadow),
            },
            color(0, 0, 0, 255),
        )?;
    }
    put(
        Point {
            x: float(x),
            y: float(y),
        },
        label.color,
    )
}
