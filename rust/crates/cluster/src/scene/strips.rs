use super::{
    dashes::{dashed_centerline_segments, LANE_DASH_GAP_M, LANE_DASH_LENGTH_M},
    math::normalize2,
    Color, MeshStrip, Vec3,
};
use num_traits::ToPrimitive;
use std::sync::Arc;

pub type MeshStripGroups = Arc<[Vec<MeshStrip>]>;
pub type WidthSpec = (f64, Color, f64);
pub type MarkingSpec = (i64, Color, f64);

#[must_use]
pub fn strips_from_centerline_width_specs(points: &[Vec3], specs: &[WidthSpec]) -> Vec<MeshStrip> {
    if points.len() < 2 {
        return Vec::new();
    }
    let mut sides: Vec<(Vec<Vec3>, Vec<Vec3>)> = specs
        .iter()
        .map(|_| {
            (
                Vec::with_capacity(points.len()),
                Vec::with_capacity(points.len()),
            )
        })
        .collect();
    for (index, point) in points.iter().enumerate() {
        let previous = points[index.saturating_sub(1)];
        let next = points[(index + 1).min(points.len() - 1)];
        let (tangent_x, tangent_y) = normalize2(next.x - previous.x, next.y - previous.y);
        for ((width, _, height), (left, right)) in specs.iter().zip(&mut sides) {
            let half = 0.001_f64.max(*width) * 0.5;
            left.push(Vec3::new(
                point.x - tangent_y * half,
                point.y + tangent_x * half,
                *height,
            ));
            right.push(Vec3::new(
                point.x + tangent_y * half,
                point.y - tangent_x * half,
                *height,
            ));
        }
    }
    sides
        .into_iter()
        .zip(specs)
        .map(|((left, right), (_, color, _))| MeshStrip::new(left, right, *color))
        .collect()
}

struct StripGroups {
    sides: Vec<(Vec<Vec3>, Vec<Vec3>)>,
    widths: Vec<f64>,
}
impl StripGroups {
    fn new(specs: &[MarkingSpec]) -> Self {
        Self {
            sides: specs.iter().map(|_| (Vec::new(), Vec::new())).collect(),
            widths: specs
                .iter()
                .map(|(width, _, _)| {
                    0.08_f64.max(width.to_f64().expect("i64 is representable as f64") * 0.022) * 0.5
                })
                .collect(),
        }
    }
    fn append(&mut self, points: &[Vec3], specs: &[MarkingSpec]) {
        if points.len() < 2 {
            return;
        }
        for (index, point) in points.iter().enumerate() {
            let previous = points[index.saturating_sub(1)];
            let next = points[(index + 1).min(points.len() - 1)];
            let dx = next.x - previous.x;
            let dy = next.y - previous.y;
            let length = (dx * dx + dy * dy).sqrt();
            let (right_x, right_y) = if length <= 0.0001 {
                (1.0, -0.0)
            } else {
                let inverse = 1.0 / length;
                (dy * inverse, -dx * inverse)
            };
            for ((half, (_, _, height)), (left_group, right_group)) in
                self.widths.iter().zip(specs).zip(&mut self.sides)
            {
                let left = Vec3::new(point.x - right_x * half, point.y - right_y * half, *height);
                let right = Vec3::new(point.x + right_x * half, point.y + right_y * half, *height);
                if index == 0 {
                    if let Some(&previous_right) = right_group.last() {
                        left_group.extend([previous_right, left]);
                        right_group.extend([previous_right, left]);
                    }
                }
                left_group.push(left);
                right_group.push(right);
            }
        }
    }
    fn finish(self, specs: &[MarkingSpec]) -> MeshStripGroups {
        self.sides
            .into_iter()
            .zip(specs)
            .map(|((left, right), (_, color, _))| {
                if left.len() >= 2 && right.len() >= 2 {
                    vec![MeshStrip::new(left, right, *color)]
                } else {
                    Vec::new()
                }
            })
            .collect::<Vec<_>>()
            .into()
    }
}

#[must_use]
pub fn lane_marking_strip_groups_from_segments(
    segments: &[Vec<Vec3>],
    specs: &[MarkingSpec],
) -> MeshStripGroups {
    let mut groups = StripGroups::new(specs);
    for segment in segments {
        groups.append(segment, specs);
    }
    groups.finish(specs)
}

#[must_use]
pub fn lane_marking_strip_groups_from_centerline(
    points: &[Vec3],
    specs: &[MarkingSpec],
    style: &str,
) -> MeshStripGroups {
    let mut groups = StripGroups::new(specs);
    if style == "solid" {
        groups.append(points, specs);
    } else {
        for segment in dashed_centerline_segments(points, LANE_DASH_LENGTH_M, LANE_DASH_GAP_M) {
            groups.append(&segment, specs);
        }
    }
    groups.finish(specs)
}

#[must_use]
pub fn merge_mesh_strip(strips: &[MeshStrip]) -> Option<MeshStrip> {
    let first = strips.first()?;
    if strips.len() == 1 {
        return Some(first.clone());
    }
    let mut left = Vec::new();
    let mut right = Vec::new();
    for strip in strips {
        let count = strip.left.len().min(strip.right.len());
        if count < 2 {
            continue;
        }
        if let Some(&previous_right) = right.last() {
            left.extend([previous_right, strip.left[0]]);
            right.extend([previous_right, strip.left[0]]);
        }
        left.extend_from_slice(&strip.left[..count]);
        right.extend_from_slice(&strip.right[..count]);
    }
    if left.len() < 2 || right.len() < 2 {
        return None;
    }
    Some(MeshStrip {
        left: left.into(),
        right: right.into(),
        color: first.color,
        x_offset_m: first.x_offset_m,
    })
}
