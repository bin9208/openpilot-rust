use super::{
    math::{float, interp},
    points::{ModelPoint, SamplePoint, ScreenPoint},
};
use crate::Error;
use openpilot_ui_framework::geometry::{Point, Rect};
use serde::Serialize;

#[derive(Clone, Debug, Default, Serialize)]
pub struct Projection {
    pub transform: [[f32; 3]; 3],
    pub clip: Rect,
}
#[derive(Clone, Copy, Debug)]
pub struct Ribbon {
    pub half_width: f64,
    pub height: f64,
    pub shift: f64,
    pub end: usize,
    pub end_distance: Option<f64>,
    pub allow_invert: bool,
}
impl Projection {
    pub fn set_transform(&mut self, matrix: [[f64; 3]; 3]) {
        self.transform = matrix.map(|row| row.map(float));
    }
    pub fn contains(&self, point: ScreenPoint) -> bool {
        let [x, y] = point.0;
        x >= f64::from(self.clip.x)
            && x <= f64::from(self.clip.x) + f64::from(self.clip.width)
            && y >= f64::from(self.clip.y)
            && y <= f64::from(self.clip.y) + f64::from(self.clip.height)
    }
    pub fn point(&self, point: SamplePoint) -> Option<ScreenPoint> {
        let [x, y, z] = point.0;
        // NumPy's single-vector dgemv accumulates columns 1,0,2 on this source backend.
        let projected = std::array::from_fn::<_, 3, _>(|i| {
            let [a, b, c] = self.transform[i].map(f64::from);
            c.mul_add(z, a.mul_add(x, b * y))
        });
        if projected[2].abs() < 1e-6 {
            return None;
        }
        let screen = ScreenPoint([projected[0] / projected[2], projected[1] / projected[2]]);
        self.contains(screen).then_some(screen)
    }
    pub fn batch_float(&self, point: ModelPoint) -> [f32; 3] {
        let [x, y, z] = point.0;
        self.transform
            .map(|[a, b, c]| c.mul_add(z, b.mul_add(y, a * x)))
    }
    pub fn batch_double(&self, point: SamplePoint) -> [f64; 3] {
        let [x, y, z] = point.0;
        self.transform.map(|row| {
            let [a, b, c] = row.map(f64::from);
            c.mul_add(z, b.mul_add(y, a * x))
        })
    }
    pub fn explicit_float(&self, point: ModelPoint) -> [f32; 3] {
        let [x, y, z] = point.0;
        self.transform.map(|[a, b, c]| (a * x + b * y) + c * z)
    }
    pub fn pair_float(&self, left: [f32; 3], right: [f32; 3]) -> Option<[Point; 2]> {
        if !(left[2].abs() >= 1e-6 && right[2].abs() >= 1e-6) {
            return None;
        }
        let pair = [left, right].map(|p| Point {
            x: p[0] / p[2],
            y: p[1] / p[2],
        });
        pair.iter()
            .all(|p| self.contains(ScreenPoint([f64::from(p.x), f64::from(p.y)])))
            .then_some(pair)
    }
    pub fn pair_double(&self, left: [f64; 3], right: [f64; 3]) -> Option<[ScreenPoint; 2]> {
        if !(left[2].abs() >= 1e-6 && right[2].abs() >= 1e-6) {
            return None;
        }
        let pair = [left, right].map(|p| ScreenPoint([p[0] / p[2], p[1] / p[2]]));
        pair.iter().all(|p| self.contains(*p)).then_some(pair)
    }
    pub fn ribbon(&self, line: &[ModelPoint], shape: Ribbon) -> Result<Vec<Point>, Error> {
        let mut points = line[..line.len().min(shape.end.saturating_add(1))].to_vec();
        if let Some(distance) = shape.end_distance {
            if shape.end > 0 && shape.end < line.len().saturating_sub(1) {
                let (a, b) = (line[shape.end].0, line[shape.end + 1].0);
                let nodes = [f64::from(a[0]), f64::from(b[0])];
                points.push(ModelPoint([
                    float(distance),
                    float(interp(
                        distance,
                        &nodes,
                        &[f64::from(a[1]), f64::from(b[1])],
                    )?),
                    float(interp(
                        distance,
                        &nodes,
                        &[f64::from(a[2]), f64::from(b[2])],
                    )?),
                ]));
            }
        }
        let offsets = [
            float(-shape.half_width + shape.shift),
            float(shape.half_width + shape.shift),
        ];
        let height = float(shape.height);
        let mut pairs = Vec::with_capacity(points.len());
        let mut lowest = f32::INFINITY;
        for point in points.into_iter().filter(|p| p.0[0] >= 0.) {
            let side = offsets.map(|y| {
                self.batch_float(ModelPoint([
                    point.0[0],
                    point.0[1] + y,
                    point.0[2] + height,
                ]))
            });
            if let Some(pair) = self.pair_float(side[0], side[1]) {
                if shape.allow_invert || pair[0].y <= lowest {
                    lowest = pair[0].y;
                    pairs.push(pair);
                }
            }
        }
        Ok(ribbon_vertices(&pairs))
    }
}
pub fn ribbon_vertices(pairs: &[[Point; 2]]) -> Vec<Point> {
    pairs
        .iter()
        .map(|p| p[0])
        .chain(pairs.iter().rev().map(|p| p[1]))
        .collect()
}
