use super::{
    drawing::{self, PathPaint},
    math::float,
    ModelRenderer,
};
use crate::Error;
use openpilot_ui_framework::{draw::Draw, geometry::Point};
impl ModelRenderer {
    pub(super) fn draw_special(&self, draw: &mut dyn Draw, paint: PathPaint) -> Result<(), Error> {
        let vertices = &self.common.path.projected;
        let length = vertices.len();
        if length < 4 {
            return Ok(());
        }
        let (g, gc) = match self.carrot.mode {
            13 => (0.2, 0.1),
            14 => (0.45, 0.05),
            15 => (0.05, 0.05),
            _ => (0.05, 0.4),
        };
        let count = length / 2 - 1;
        let mut strips = std::array::from_fn::<_, 3, _>(|_| vec![Point::default(); count * 2]);
        for i in 0..count {
            let a = vertices[i];
            let b = vertices[length - i - 1];
            let end = count * 2 - 1 - i;
            let mix = |t| Point {
                x: float(f64::from(a.x) + (f64::from(b.x) - f64::from(a.x)) * t),
                y: float(f64::from(a.y) + (f64::from(b.y) - f64::from(a.y)) * t),
            };
            strips[0][i] = a;
            strips[0][end] = mix(g);
            strips[1][i] = mix(0.5 - gc);
            strips[1][end] = mix(0.5 + gc);
            strips[2][i] = mix(1. - g);
            strips[2][end] = b;
        }
        if matches!(self.carrot.mode, 13 | 14) {
            drawing::path_polygon(draw, &strips[0], paint)?;
        }
        if matches!(self.carrot.mode, 13 | 15) {
            drawing::path_polygon(draw, &strips[1], paint)?;
        }
        if matches!(self.carrot.mode, 13 | 14) {
            drawing::path_polygon(draw, &strips[2], paint)?;
        }
        Ok(())
    }
    pub(super) fn draw_complex(&self, draw: &mut dyn Draw, paint: PathPaint) -> Result<(), Error> {
        let points = &self.common.path.projected;
        let length = points.len();
        if length < 6 {
            return Ok(());
        }
        for i in (0..length / 2 - 1).step_by(3) {
            let e = length - i - 1;
            let quad = [
                points[i],
                points[i + 1],
                midpoint(points[i + 2], points[e - 2]),
                points[e - 1],
                points[e],
                midpoint(points[i + 1], points[e - 1]),
            ];
            drawing::two_quads(draw, &quad, paint)?;
        }
        Ok(())
    }
}
pub(super) fn midpoint(a: Point, b: Point) -> Point {
    Point {
        x: float((f64::from(a.x) + f64::from(b.x)) / 2.),
        y: float((f64::from(a.y) + f64::from(b.y)) / 2.),
    }
}
