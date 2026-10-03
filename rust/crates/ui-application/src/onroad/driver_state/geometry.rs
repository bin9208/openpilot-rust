use openpilot_ui_framework::{
    geometry::{Point, Rect},
    text_layout::float,
};
use serde::Serialize;
#[derive(Clone, Serialize)]
pub struct Arc {
    pub points: Vec<Point>,
    pub thickness: f64,
}
#[derive(Default, Serialize)]
pub struct Geometry {
    pub fade: f64,
    pub pose: [f64; 3],
    pub difference: [f64; 3],
    pub sins: [f64; 3],
    pub coss: [f64; 3],
    pub face: Vec<[f64; 3]>,
    pub transformed: Vec<[f64; 2]>,
    pub lines: Vec<Point>,
    pub center: [f64; 2],
    pub horizontal: Option<Arc>,
    pub vertical: Option<Arc>,
}
impl Geometry {
    pub fn update(&mut self, orientation: [f64; 3], active: bool, rhd: bool, rect: Rect) {
        self.fade =
            (self.fade + 0.2 * (if active { 0.0 } else { 0.5 } - self.fade)).clamp(0.0, 1.0);
        for (i, orient) in orientation.into_iter().enumerate() {
            let scale = if i == 0 {
                if orient < 0.0 {
                    0.7_f32
                } else {
                    0.9_f32
                }
            } else {
                0.4_f32
            };
            let value = orient * f64::from(scale);
            self.difference[i] = (self.pose[i] - value).abs();
            self.pose[i] = 0.8 * value + 0.2 * self.pose[i];
            let rotation = self.pose[i] * (1.0 - self.fade);
            self.sins[i] = rotation.sin();
            self.coss[i] = rotation.cos();
        }
        let [sy, sx, sz] = self.sins;
        let [cy, cx, cz] = self.coss;
        let rotation = [
            [cx * cz, cx * sz, -sx],
            [-sy * sx * cz - cy * sz, -sy * sx * sz + cy * cz, -sy * cx],
            [cy * sx * cz - sy * sz, cy * sx * sz + sy * cz, cy * cx],
        ];
        self.face = super::points::FACE
            .iter()
            .map(|p| {
                let mut result =
                    rotation.map(|r| (0..3).map(|i| f64::from(p[i]) * r[i]).sum::<f64>());
                result[2] = result[2] * (1.0 - self.fade) + 8.0 * self.fade;
                result
            })
            .collect();
        self.transformed = self
            .face
            .iter()
            .map(|p| {
                let depth = (p[2] - 8.0) / 120.0 + 1.0;
                [p[0] * depth, p[1] * depth]
            })
            .collect();
        self.center = [
            f64::from(rect.x)
                + if rhd {
                    f64::from(rect.width) - 126.0
                } else {
                    126.0
                },
            f64::from(rect.y) + f64::from(rect.height) - 126.0,
        ];
        self.lines = self
            .transformed
            .iter()
            .map(|p| Point {
                x: float(p[0] + self.center[0]),
                y: float(p[1] + self.center[1]),
            })
            .collect();
        self.horizontal = arc(self.sins[1], self.difference[1], self.center, true);
        self.vertical = arc(self.sins[0], self.difference[0], self.center, false);
    }
}
fn arc(sin: f64, difference: f64, center: [f64; 2], horizontal: bool) -> Option<Arc> {
    let delta = -sin * 133.0 / 2.0;
    let size = delta.abs();
    if size <= 0.0 {
        return None;
    }
    let thickness = 6.7 + 12.0 * (difference * 5.0).min(1.0);
    let start: f64 = if horizontal {
        if sin > 0.0 {
            90.0
        } else {
            -90.0
        }
    } else if sin > 0.0 {
        0.0
    } else {
        180.0
    };
    let (x, y, w, h) = if horizontal {
        (
            (center[0] + delta).min(center[0]),
            center[1] - 66.5,
            size,
            133.0,
        )
    } else {
        (
            center[0] - 66.5,
            (center[1] + delta).min(center[1]),
            133.0,
            size,
        )
    };
    let points = (0..37)
        .map(|i| {
            let angle =
                f64::from(float(f64::from(i) * (std::f64::consts::PI / 36.0))) + start.to_radians();
            Point {
                x: float(x + w / 2.0 + angle.cos() * (w / 2.0)),
                y: float(y + h / 2.0 - angle.sin() * (h / 2.0)),
            }
        })
        .collect();
    Some(Arc { points, thickness })
}
