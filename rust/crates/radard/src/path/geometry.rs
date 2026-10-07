use super::Projection;
use crate::math::{finite, maximum, minimum, norm};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Geometry {
    pub points: Vec<[f64; 2]>,
    pub segments: Vec<[f64; 6]>,
}

impl Geometry {
    pub fn new(input: &[[f64; 2]]) -> Self {
        let points: Vec<_> = input
            .iter()
            .map(|[x, y]| [finite(*x, 0.), -finite(*y, 0.)])
            .collect();
        let mut segments = Vec::with_capacity(points.len().saturating_sub(1));
        let mut accumulated = 0.;
        for pair in points.windows(2) {
            let [x, y] = pair[0];
            let dx = pair[1][0] - x;
            let dy = pair[1][1] - y;
            let length = norm(&[dx, dy]);
            if length < 1e-6 {
                continue;
            }
            segments.push([x, y, dx / length, dy / length, length, accumulated]);
            accumulated += length;
        }
        Self { points, segments }
    }

    pub fn project(&self, x: f64, y: f64) -> Projection {
        let [center_x, center_y] = self.points[0];
        let mut best = Projection {
            path_s: x - center_x,
            center_x,
            center_y,
            tangent_x: 1.,
            tangent_y: 0.,
            d_path: y - center_y,
        };
        let mut distance = f64::INFINITY;
        for &[x0, y0, tangent_x, tangent_y, length, accumulated] in &self.segments {
            let dx = tangent_x * length;
            let dy = tangent_y * length;
            let raw_ratio = ((x - x0) * dx + (y - y0) * dy) / (length * length);
            let ratio = minimum(1., maximum(0., raw_ratio));
            let center_x = x0 + ratio * dx;
            let center_y = y0 + ratio * dy;
            let offset_x = x - center_x;
            let offset_y = y - center_y;
            let candidate_distance = offset_x * offset_x + offset_y * offset_y;
            if candidate_distance < distance {
                distance = candidate_distance;
                best = Projection {
                    path_s: accumulated + ratio * length,
                    center_x,
                    center_y,
                    tangent_x,
                    tangent_y,
                    d_path: -tangent_y * offset_x + tangent_x * offset_y,
                };
            }
        }
        best
    }

    pub fn at(&self, distance: f64, offset: f64) -> [f64; 2] {
        if self.segments.is_empty() {
            return [self.points[0][0] + distance, self.points[0][1] + offset];
        }
        for (index, &[x, y, tangent_x, tangent_y, length, accumulated]) in
            self.segments.iter().enumerate()
        {
            let mut relative = distance - accumulated;
            if relative <= length || index == self.segments.len() - 1 {
                if index > 0 {
                    relative = maximum(0., relative);
                }
                return [
                    x + relative * tangent_x - tangent_y * offset,
                    y + relative * tangent_y + tangent_x * offset,
                ];
            }
        }
        self.points[self.points.len() - 1]
    }
}
