//! Ribbon polygon triangulation and gradient policy from shader_polygon.py (MIT).
use crate::{
    draw::{Draw, PolygonPaint},
    geometry::{Point, Rect},
    text_layout::float,
    Error,
};
use num_traits::ToPrimitive;
#[derive(Clone, Debug)]
pub struct Gradient {
    pub start: (f64, f64),
    pub end: (f64, f64),
    pub colors: Vec<u32>,
    pub stops: Vec<f64>,
}
impl Gradient {
    pub fn new(
        start: (f64, f64),
        end: (f64, f64),
        mut colors: Vec<u32>,
        mut stops: Vec<f64>,
    ) -> Self {
        if colors.len() > 20 {
            colors.truncate(20);
            eprintln!("Warning: Gradient colors truncated to 20 entries");
        }
        if stops.len() > 20 {
            stops.truncate(20);
            eprintln!("Warning: Gradient stops truncated to 20 entries");
        }
        if stops.is_empty() {
            let count = colors
                .len()
                .saturating_sub(1)
                .max(1)
                .to_f64()
                .unwrap_or(1.0);
            stops = (0..colors.len())
                .map(|index| index.to_f64().unwrap_or(0.0) / count)
                .collect();
        }
        Self {
            start,
            end,
            colors,
            stops,
        }
    }
}
pub fn triangulate(points: &[Point]) -> Vec<Point> {
    let count = points.len() - points.len() % 2;
    let mut strip = Vec::with_capacity(count);
    for index in 0..count / 2 {
        strip.push(points[index]);
        strip.push(points[count - index - 1]);
    }
    strip
}
pub enum Fill<'a> {
    Color(u32),
    Gradient(&'a Gradient),
}
pub fn polygon(
    draw: &mut dyn Draw,
    points: &[Point],
    (origin, fill): (Rect, Fill<'_>),
) -> Result<(), Error> {
    if points.len() < 3 {
        return Ok(());
    }
    let strip = triangulate(points);
    match fill {
        Fill::Color(color) => draw.shaded_strip(&strip, PolygonPaint::Color(color)),
        Fill::Gradient(gradient) => {
            if gradient.colors.is_empty() {
                return Err(Error::Contract(
                    "empty gradient is unsupported by the source renderer",
                ));
            }

            let stops: Vec<_> = gradient
                .stops
                .iter()
                .map(|value| float(value.clamp(0.0, 1.0)))
                .collect();
            let position = |value: (f64, f64)| Point {
                x: float(f64::from(origin.x) + value.0 * f64::from(origin.width)),
                y: float(f64::from(origin.y) + value.1 * f64::from(origin.height)),
            };
            draw.shaded_strip(
                &strip,
                PolygonPaint::Gradient {
                    start: position(gradient.start),
                    end: position(gradient.end),
                    colors: &gradient.colors,
                    stops: &stops,
                },
            )
        }
    }
}
pub fn solid(draw: &mut dyn Draw, points: &[Point], color: u32) -> Result<(), Error> {
    if points.len() < 3 {
        return Ok(());
    }
    draw.triangle_strip(&triangulate(points), color)
}
