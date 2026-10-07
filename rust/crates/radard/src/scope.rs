use crate::{
    math::{finite, minimum},
    path::{Path, Projection},
    point::{Identity, Point},
};
use std::collections::HashSet;

#[derive(Clone, Copy)]
pub struct Scoped<'a> {
    pub point: &'a Point,
    pub distance: f64,
    pub projection: Projection,
}

pub fn points<'a>(points: &'a [Point], path: &Path) -> Vec<Scoped<'a>> {
    points
        .iter()
        .filter_map(|point| {
            if !point.measured {
                return None;
            }
            let distance = finite(point.d_rel, 0.);
            if !(-5. ..=100.).contains(&distance) {
                return None;
            }
            let lateral = finite(point.y_rel, 0.);
            let projection = path.project(distance, lateral);
            let separation = crate::math::norm(&[
                distance - projection.center_x,
                lateral - projection.center_y,
            ]);
            (separation <= 1.5 * 3.60).then_some(Scoped {
                point,
                distance,
                projection,
            })
        })
        .collect()
}

pub fn visible<'a>(
    scoped: &[Scoped<'a>],
    primary_distance: Option<f64>,
    protected: &HashSet<Identity>,
) -> Vec<&'a Point> {
    let overlap = 1.8 + 0.12;
    let primary = primary_distance.filter(|value| value.is_finite() && *value > 0.);
    let mut nearest = [f64::INFINITY; 2];
    for value in scoped {
        if value.distance >= 5. && value.projection.d_path.abs() > overlap {
            let side = usize::from(value.projection.d_path > 0.);
            nearest[side] = minimum(nearest[side], value.distance);
        }
    }
    scoped
        .iter()
        .filter(|value| {
            value.distance < 5.
                || value.projection.d_path.abs() <= overlap
                || protected.contains(&value.point.identity())
                || primary.is_some_and(|distance| value.distance < distance)
                || value.distance <= nearest[usize::from(value.projection.d_path > 0.)] + 1.
        })
        .map(|value| value.point)
        .collect()
}

pub fn turning_entry_allowed(point: &Point, d_path: f64, yaw: f64, cross_sensor: bool) -> bool {
    if cross_sensor || !point.corner() {
        return true;
    }
    if ![point.y_rel, d_path, yaw]
        .iter()
        .all(|value| value.is_finite())
    {
        return true;
    }
    !(yaw.abs() >= 0.20 && point.y_rel.abs() >= 5. && point.y_rel.abs() - d_path.abs() >= 3.)
}
