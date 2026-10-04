use super::{
    math::{float, integer},
    points::ScreenPoint,
};
use crate::{paint::color, Error};
use openpilot_ui_framework::{
    draw::{Draw, RoundedOutline, TextDraw},
    geometry::{Point, Rect},
    polygon,
    text::Font,
    text_layout,
};

pub fn point(ScreenPoint([x, y]): ScreenPoint) -> Point {
    Point {
        x: float(x),
        y: float(y),
    }
}
pub fn outline(draw: &mut dyn Draw, points: &[Point], style: (u32, f32)) -> Result<(), Error> {
    if points.len() < 2 {
        return Ok(());
    }
    for pair in points.windows(2) {
        draw.line(pair[0], pair[1], style.1, style.0)?;
    }
    draw.line(points[points.len() - 1], points[0], style.1, style.0)?;
    Ok(())
}
#[derive(Clone, Copy)]
pub struct PathPaint {
    pub fill: u32,
    pub brake: bool,
    pub color_index: i32,
}
pub fn path_polygon(draw: &mut dyn Draw, points: &[Point], paint: PathPaint) -> Result<(), Error> {
    let mut clean = Vec::<Point>::with_capacity(points.len());
    for &p in points {
        if clean.last().is_none_or(|last| {
            f64::from(p.x - last.x).abs() > 1e-3 || f64::from(p.y - last.y).abs() > 1e-3
        }) {
            clean.push(p);
        }
    }
    if clean.len() >= 3
        && f64::from(clean[0].x - clean[clean.len() - 1].x).abs() < 1e-3
        && f64::from(clean[0].y - clean[clean.len() - 1].y).abs() < 1e-3
    {
        clean.pop();
    }
    if clean.len() < 3 {
        return Ok(());
    }
    polygon::solid(draw, &clean, paint.fill)?;
    path_outline(draw, &clean, &paint)
}
pub fn path_outline(draw: &mut dyn Draw, points: &[Point], paint: &PathPaint) -> Result<(), Error> {
    if paint.color_index >= 10 || paint.brake {
        outline(
            draw,
            points,
            (
                if paint.brake {
                    color(255, 0, 0, 255)
                } else {
                    color(255, 255, 255, 255)
                },
                2.,
            ),
        )?;
    }
    Ok(())
}
pub fn two_quads(draw: &mut dyn Draw, points: &[Point; 6], paint: PathPaint) -> Result<(), Error> {
    polygon::solid(
        draw,
        &[points[0], points[1], points[2], points[5]],
        paint.fill,
    )?;
    polygon::solid(
        draw,
        &[points[5], points[2], points[3], points[4]],
        paint.fill,
    )?;
    path_outline(draw, points, &paint)
}
pub struct BoxStyle {
    pub fill: u32,
    pub stroke: u32,
    pub thickness: f32,
}
pub fn rounded_box(draw: &mut dyn Draw, rect: Rect, style: BoxStyle) -> Result<(), Error> {
    draw.rounded_segments(rect, 0.15, 12, style.fill, false)?;
    if style.thickness > 0. {
        draw.rounded_outline(
            rect,
            RoundedOutline {
                roundness: 0.15,
                segments: 12,
                thickness: style.thickness,
                color: style.stroke,
            },
        )?;
    }
    Ok(())
}
#[derive(Clone, Copy)]
pub enum Anchor {
    Center,
    LeftTop,
}
pub struct Label<'a> {
    pub value: &'a str,
    pub position: ScreenPoint,
    pub size: f64,
    pub color: u32,
    pub anchor: Anchor,
    pub border: f64,
}
impl<'a> Label<'a> {
    pub fn center(value: &'a str, position: ScreenPoint, size: f64) -> Self {
        Self {
            value,
            position,
            size,
            color: color(255, 255, 255, 255),
            anchor: Anchor::Center,
            border: 3.,
        }
    }
}
pub fn text(draw: &mut dyn Draw, label: Label<'_>) -> Result<(), Error> {
    if label.value.is_empty() {
        return Ok(());
    }
    let measured = text_layout::measure(draw, Font::Display, label.value, label.size, 0.);
    let [mut x, mut y] = label.position.0;
    match label.anchor {
        Anchor::Center => {
            x -= f64::from(measured.x) * 0.5;
            y -= f64::from(measured.y) * 0.5;
        }
        Anchor::LeftTop => {}
    }
    let size = float(label.size * draw.font_scale());
    let mut put = |position, color| {
        draw.text(TextDraw {
            font: Font::Display,
            text: label.value,
            position,
            size,
            spacing: 0.,
            color,
        })
    };
    if label.border > 0. {
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
    put(
        Point {
            x: float(x + 8.),
            y: float(y + 8.),
        },
        color(0, 0, 0, 255),
    )?;
    put(
        Point {
            x: float(x),
            y: float(y),
        },
        label.color,
    )?;
    Ok(())
}
pub fn text_box(draw: &mut dyn Draw, label: Label<'_>, background: u32) -> Result<(), Error> {
    use num_traits::ToPrimitive;
    let chars = label
        .value
        .chars()
        .count()
        .to_f64()
        .ok_or(Error::Contract("label length"))?;
    let width = integer(chars * label.size * 0.8)?.max(40);
    let [x, y] = label.position.0;
    rounded_box(
        draw,
        Rect {
            x: float(x - f64::from(width) / 2.),
            y: float(y - 20.),
            width: float(f64::from(width)),
            height: 42.,
        },
        BoxStyle {
            fill: background,
            stroke: background,
            thickness: 0.,
        },
    )?;
    text(draw, label)
}
