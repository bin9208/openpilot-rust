use super::{constants::*, source::position_continuous, Matcher};
use crate::{
    math::{maximum, minimum},
    model::VisionLead,
    path::Path,
    point::{Identity, Point},
};
use indexmap::IndexMap;
use std::collections::HashSet;

pub type Pair = (Point, f64, Point, f64);

pub fn equivalent(first: &Point, second: &Point) -> bool {
    first.source == "frontRadar"
        && second.corner()
        && (first.d_rel - second.d_rel).abs() <= STATIONARY_VISION_CROSS_SOURCE_MAX_DREL_M
        && (first.y_rel - second.y_rel).abs() <= STATIONARY_VISION_CROSS_SOURCE_MAX_YREL_M
        && (first.v_lead - second.v_lead).abs() <= STATIONARY_VISION_CROSS_SOURCE_MAX_VLEAD_MPS
}

pub fn front_corner_pairs(points: &[Point], path: &Path) -> Vec<Pair> {
    let fronts = points.iter().filter(|point| {
        point.measured
            && point.source == "frontRadar"
            && 0.5 < point.d_rel
            && point.d_rel < 180.
            && point.v_lead.abs() <= STATIONARY_MAX_ABS_VLEAD_MPS
    });
    let corners: Vec<_> = points
        .iter()
        .filter(|point| {
            point.measured
                && point.corner()
                && 0.5 < point.d_rel
                && point.d_rel < 180.
                && point.v_lead.abs() <= STATIONARY_VISION_CROSS_SOURCE_CORNER_MAX_ABS_VLEAD_MPS
        })
        .collect();
    let mut pairs = Vec::new();
    let mut projected = IndexMap::new();
    for front in fronts {
        let mut matches: Vec<_> = corners
            .iter()
            .copied()
            .filter(|corner| equivalent(front, corner))
            .collect();
        if matches.is_empty() {
            continue;
        }
        let front_d_path = path.project(front.d_rel, front.y_rel).d_path;
        if front_d_path.abs() > STATIONARY_FRESH_MAX_DPATH_M {
            continue;
        }
        matches.sort_by(|left, right| {
            (
                (front.d_rel - left.d_rel).abs(),
                (front.y_rel - left.y_rel).abs(),
                left.track_id,
            )
                .partial_cmp(&(
                    (front.d_rel - right.d_rel).abs(),
                    (front.y_rel - right.y_rel).abs(),
                    right.track_id,
                ))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for corner in matches {
            let d_path = *projected
                .entry(corner.identity())
                .or_insert_with(|| path.project(corner.d_rel, corner.y_rel).d_path);
            if d_path.abs() <= STATIONARY_FRESH_MAX_DPATH_M {
                pairs.push((front.clone(), front_d_path, corner.clone(), d_path));
                break;
            }
        }
    }
    pairs
}

pub fn cross_position_cost(vision: Option<VisionLead>, front: &Point) -> Option<f64> {
    let vision = vision?;
    if vision.probability < STATIONARY_VISION_MIN_PROB {
        return None;
    }
    let distance_gate = minimum(
        STATIONARY_VISION_CROSS_SOURCE_MAX_DISTANCE_ERROR_M,
        maximum(
            maximum(
                VISION_RADAR_MAX_DISTANCE_ERROR_M,
                vision.d_rel * STATIONARY_VISION_DISTANCE_FRACTION,
            ),
            minimum(
                STATIONARY_VISION_CROSS_SOURCE_MAX_DISTANCE_ERROR_M,
                vision.x_std.abs() * 3.,
            ),
        ),
    );
    let lateral_gate = maximum(2., minimum(4., vision.y_std.abs() * 3.));
    let distance_error = (front.d_rel - vision.d_rel).abs();
    let lateral_error = (front.y_rel - vision.y_rel).abs();
    if distance_error > distance_gate || lateral_error > lateral_gate {
        None
    } else {
        Some(distance_error / distance_gate + lateral_error / lateral_gate)
    }
}

pub fn cross_front_support(
    vision: Option<VisionLead>,
    pairs: &[Pair],
) -> Vec<(Point, f64, f64, Point)> {
    let Some(vision) = vision else {
        return Vec::new();
    };
    if vision.probability < STATIONARY_VISION_MIN_PROB {
        return Vec::new();
    }
    pairs
        .iter()
        .filter_map(|(front, d_path, corner, _)| {
            let position_cost = cross_position_cost(Some(vision), front)?;
            if (front.v_lead - vision.velocity).abs() > STATIONARY_MAX_VISION_SPEED_DELTA_MPS {
                return None;
            }
            let cost = position_cost
                + (front.d_rel - corner.d_rel).abs() / STATIONARY_VISION_CROSS_SOURCE_MAX_DREL_M
                + (front.y_rel - corner.y_rel).abs() / STATIONARY_VISION_CROSS_SOURCE_MAX_YREL_M
                + (front.v_lead - corner.v_lead).abs()
                    / STATIONARY_VISION_CROSS_SOURCE_MAX_VLEAD_MPS;
            Some((front.clone(), *d_path, cost, corner.clone()))
        })
        .collect()
}

pub fn vision_base_cost(vision: Option<VisionLead>, point: &Point) -> Option<f64> {
    let vision = vision?;
    let speed_limit = if point.corner() || point.source == "scc" {
        STATIONARY_TRUSTED_MAX_VISION_SPEED_DELTA_MPS
    } else {
        STATIONARY_MAX_VISION_SPEED_DELTA_MPS
    };
    if vision.probability < STATIONARY_VISION_MIN_PROB
        || (point.v_lead - vision.velocity).abs() > speed_limit
    {
        return None;
    }
    let distance_gate = maximum(
        maximum(
            6.,
            minimum(
                STATIONARY_VISION_DISTANCE_MAX_M,
                vision.d_rel * STATIONARY_VISION_DISTANCE_FRACTION,
            ),
        ),
        minimum(STATIONARY_VISION_DISTANCE_MAX_M, vision.x_std.abs() * 3.),
    );
    let lateral_gate = maximum(2., minimum(4., vision.y_std.abs() * 3.));
    let distance_error = (point.d_rel - vision.d_rel).abs();
    let lateral_error = (point.y_rel - vision.y_rel).abs();
    if distance_error > distance_gate || lateral_error > lateral_gate {
        None
    } else {
        Some(distance_error / distance_gate + lateral_error / lateral_gate)
    }
}

pub fn vision_cost(
    vision: VisionLead,
    point: &Point,
    d_path: f64,
    prefer_corner: bool,
) -> Option<f64> {
    Some(
        vision_base_cost(Some(vision), point)? + 0.15 * d_path.abs() / STATIONARY_FRESH_MAX_DPATH_M
            - if prefer_corner && point.corner() {
                0.35
            } else {
                0.
            },
    )
}

pub fn corroborating_fronts(points: &[Point], projected: &[(Point, f64)]) -> Vec<Point> {
    let corners: Vec<_> = projected
        .iter()
        .filter(|(point, d_path)| {
            point.corner() && d_path.abs() <= STATIONARY_RADAR_ONLY_CORNER_MAX_DPATH_M
        })
        .map(|(point, _)| point)
        .collect();
    if corners.is_empty() {
        return Vec::new();
    }
    let identities: HashSet<_> = projected
        .iter()
        .map(|(point, _)| point.identity())
        .collect();
    points
        .iter()
        .filter(|point| {
            point.source == "frontRadar"
                && !identities.contains(&point.identity())
                && corners.iter().any(|corner| {
                    (point.d_rel - corner.d_rel).abs()
                        <= STATIONARY_RADAR_ONLY_CROSS_SOURCE_MAX_DREL_M
                        && (point.v_lead - corner.v_lead).abs()
                            <= STATIONARY_RADAR_ONLY_CROSS_SOURCE_MAX_VLEAD_MPS
                })
        })
        .cloned()
        .collect()
}

impl Matcher {
    pub fn cross_source_continuous(&self, point: &Point, now: f64) -> bool {
        let Some((previous, time)) = self
            ._stationary_last_point
            .as_ref()
            .zip(self._stationary_last_time_s)
        else {
            return false;
        };
        self._stationary_corner_supported
            && point.source != previous.source
            && (point.corner() || previous.corner())
            && (point.source == "frontRadar" || point.corner())
            && (previous.source == "frontRadar" || previous.corner())
            && position_continuous(previous, time, point, now)
    }
    pub fn corner_slot_continuous(&self, point: &Point, now: f64) -> bool {
        self._stationary_last_point
            .as_ref()
            .zip(self._stationary_last_time_s)
            .is_some_and(|(previous, time)| {
                point.corner()
                    && point.source == previous.source
                    && point.identity() != previous.identity()
                    && point.v_lead.abs() <= STATIONARY_HELD_CORNER_MAX_ABS_VLEAD_MPS
                    && position_continuous(previous, time, point, now)
            })
    }
    pub fn initial_projection_points(&self, points: &[Point], now: f64) -> Vec<Point> {
        let retained: HashSet<Identity> = [
            self.stationary_identity.clone(),
            self._stationary_pending_identity.clone(),
            self._stationary_last_point.as_ref().map(Point::identity),
        ]
        .into_iter()
        .flatten()
        .collect();
        points
            .iter()
            .filter(|point| {
                point.corner()
                    || point.source == "scc"
                    || retained.contains(&point.identity())
                    || (point.source == "frontRadar" && self.cross_source_continuous(point, now))
            })
            .cloned()
            .collect()
    }
}
