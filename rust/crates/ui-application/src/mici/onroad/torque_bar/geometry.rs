use crate::Error;
use num_traits::ToPrimitive;
use openpilot_ui_framework::{geometry::Point, text_layout::float};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Clone, Copy, Deserialize, Serialize)]
pub struct Arc {
    pub cx: f64,
    pub cy: f64,
    pub radius: f64,
    pub thickness: f64,
    pub start: f64,
    pub end: f64,
}
struct Entry {
    key: [i64; 6],
    points: Vec<Point>,
}
#[derive(Default)]
pub struct ArcCache {
    entries: VecDeque<Entry>,
}
impl ArcCache {
    pub fn points(&mut self, arc: Arc) -> Result<Vec<Point>, Error> {
        let values = [
            arc.cx,
            arc.cy,
            arc.radius,
            arc.thickness,
            arc.start * 10.0,
            arc.end * 10.0,
        ];
        let mut key = [0; 6];
        for (index, value) in values.into_iter().enumerate() {
            key[index] = value
                .round_ties_even()
                .to_i64()
                .ok_or(Error::Contract("torque arc cache coordinate"))?;
        }
        let entry = if let Some(index) = self.entries.iter().position(|entry| entry.key == key) {
            self.entries
                .remove(index)
                .ok_or(Error::Contract("torque arc cache index"))?
        } else {
            if self.entries.len() >= 256 {
                self.entries.pop_front();
            }
            Entry {
                key,
                points: points(arc)?,
            }
        };
        let points = entry.points.clone();
        self.entries.push_back(entry);
        Ok(points)
    }
}
fn line(start: f64, end: f64, count: u32) -> Vec<f64> {
    let step = (end - start) / f64::from(count - 1);
    (0..count)
        .map(|index| {
            if index + 1 == count {
                end
            } else {
                start + f64::from(index) * step
            }
        })
        .collect()
}
pub fn points(mut arc: Arc) -> Result<Vec<Point>, Error> {
    if arc.end < arc.start {
        std::mem::swap(&mut arc.start, &mut arc.end);
    }
    let half = arc.thickness * 0.5;
    let cap_radius = 7.0_f64.min(half);
    let span = (arc.end - arc.start).max(1e-3);
    let segments = (arc.radius * span.to_radians() / 2.0)
        .trunc()
        .clamp(6.0, 28.0)
        .to_u32()
        .ok_or(Error::Contract("torque arc segment count"))?;
    let curve = |start: f64, end: f64, radius: f64| {
        line(start, end, segments + 1)
            .into_iter()
            .map(|angle| {
                let angle = angle.to_radians();
                [arc.cx + angle.cos() * radius, arc.cy + angle.sin() * radius]
            })
            .collect::<Vec<_>>()
    };
    let cap = |left: bool, angle: f64| {
        let angle = angle.to_radians();
        let (nx, ny) = (angle.cos(), angle.sin());
        let (tx, ty) = (-ny, nx);
        let (mx, my) = (arc.cx + nx * arc.radius, arc.cy + ny * arc.radius);
        let top = if left { (180.0, 90.0) } else { (90.0, 0.0) };
        let bottom = if left { (-90.0, -180.0) } else { (0.0, -90.0) };
        let quarter = |angles: Vec<f64>, offset: f64| {
            let (ex, ey) = (mx + nx * offset, my + ny * offset);
            angles
                .into_iter()
                .map(|alpha| {
                    let alpha = alpha.to_radians();
                    [
                        ex + alpha.cos() * cap_radius * tx + alpha.sin() * cap_radius * nx,
                        ey + alpha.cos() * cap_radius * ty + alpha.sin() * cap_radius * ny,
                    ]
                })
                .collect::<Vec<_>>()
        };
        let mut top = quarter(line(top.0, top.1, 12)[1..11].to_vec(), half - cap_radius);
        let mut bottom = quarter(
            line(bottom.0, bottom.1, 11)[..10].to_vec(),
            -half + cap_radius,
        );
        if left {
            bottom.append(&mut top);
            bottom
        } else {
            top.append(&mut bottom);
            top
        }
    };
    let mut points = curve(arc.start, arc.end, arc.radius + half);
    let first = points[0];
    points.extend(cap(false, arc.end));
    points.extend(curve(arc.end, arc.start, arc.radius - half));
    points.extend(cap(true, arc.start));
    points.push(first);
    points.rotate_right(10);
    Ok(points
        .into_iter()
        .map(|[x, y]| Point {
            x: float(x),
            y: float(y),
        })
        .collect())
}
