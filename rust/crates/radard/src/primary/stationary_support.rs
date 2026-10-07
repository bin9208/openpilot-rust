use super::{
    constants::*,
    source::position_continuous,
    stationary_geometry::{cross_position_cost, equivalent},
    Matcher, VisionMatch,
};
use crate::{
    math::{maximum, minimum},
    model::VisionLead,
    path::Path,
    point::Point,
};
use std::collections::HashSet;

pub type Candidate = (Point, f64, f64);

pub fn radar_only_support(candidates: &[(Point, f64)], yaw: f64) -> Vec<Candidate> {
    let fronts: Vec<_> = candidates
        .iter()
        .filter(|(point, _)| point.source == "frontRadar")
        .collect();
    let corners: Vec<_> = candidates
        .iter()
        .filter(|(point, d_path)| {
            point.corner()
                && point.measured
                && d_path.abs() <= STATIONARY_RADAR_ONLY_CORNER_MAX_DPATH_M
        })
        .map(|(point, _)| point)
        .collect();
    let mut supported = Vec::new();
    for (point, d_path) in candidates {
        if point.source == "frontRadar" {
            let limit = if point.d_rel >= STATIONARY_RADAR_ONLY_FRONT_FAR_DREL_M {
                STATIONARY_RADAR_ONLY_FRONT_FAR_MAX_DPATH_M
            } else {
                STATIONARY_RADAR_ONLY_FRONT_NEAR_MAX_DPATH_M
            };
            if point.measured
                && point.radar_track_state >= STATIONARY_RADAR_ONLY_FRONT_MIN_TRACK_STATE
                && point.v_lead.abs() <= STATIONARY_MAX_ABS_VLEAD_MPS
                && d_path.abs() <= limit
                && (point.y_rel - d_path).abs() <= STATIONARY_RADAR_ONLY_FRONT_MAX_PATH_Y_OFFSET_M
                && yaw.abs() < STATIONARY_RADAR_ONLY_FRONT_MAX_ABS_YAW_RATE_RAD_S
                && corners.iter().any(|corner| equivalent(point, corner))
            {
                supported.push((
                    point.clone(),
                    *d_path,
                    point.d_rel / 180. + 0.05 * d_path.abs() / limit,
                ));
            }
            continue;
        }
        if point.source == "scc" {
            if d_path.abs() <= STATIONARY_RADAR_ONLY_CORNER_MAX_DPATH_M {
                supported.push((
                    point.clone(),
                    *d_path,
                    d_path.abs() / STATIONARY_RADAR_ONLY_CORNER_MAX_DPATH_M,
                ));
            }
            continue;
        }
        if !point.corner() {
            continue;
        }
        let cross = fronts
            .iter()
            .filter_map(|(front, front_path)| {
                let value = (
                    (point.d_rel - front.d_rel).abs(),
                    (d_path - front_path).abs(),
                    (point.v_lead - front.v_lead).abs(),
                );
                (value.0 <= STATIONARY_RADAR_ONLY_CROSS_SOURCE_MAX_DREL_M
                    && value.1 <= STATIONARY_RADAR_ONLY_CROSS_SOURCE_MAX_DPATH_M
                    && value.2 <= STATIONARY_RADAR_ONLY_CROSS_SOURCE_MAX_VLEAD_MPS)
                    .then_some(value)
            })
            .min_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
        if d_path.abs() > STATIONARY_RADAR_ONLY_CORNER_MAX_DPATH_M
            || (cross.is_none()
                && point.d_rel < STATIONARY_RADAR_ONLY_CORNER_MIN_ACQUISITION_DREL_M)
        {
            continue;
        }
        let cost = cross.map_or(
            d_path.abs() / STATIONARY_RADAR_ONLY_CORNER_MAX_DPATH_M,
            |(d, y, v)| {
                d / STATIONARY_RADAR_ONLY_CROSS_SOURCE_MAX_DREL_M
                    + y / STATIONARY_RADAR_ONLY_CROSS_SOURCE_MAX_DPATH_M
                    + v / STATIONARY_RADAR_ONLY_CROSS_SOURCE_MAX_VLEAD_MPS
            },
        );
        supported.push((point.clone(), *d_path, cost));
    }
    supported
}

pub fn vision_path_compatible(vision_path: f64, radar_path: f64, held: bool) -> bool {
    let limit = if held {
        STATIONARY_HELD_MAX_DPATH_M
    } else {
        STATIONARY_FRESH_MAX_DPATH_M
    };
    vision_path.abs() <= limit && (vision_path - radar_path).abs() <= limit
}

impl Matcher {
    pub fn dropout_hold(
        &self,
        vision: Option<VisionLead>,
        points: &[Point],
        path: &Path,
        now: f64,
    ) -> Option<VisionMatch> {
        self.stationary_identity.as_ref()?;
        let (previous, time) = self
            ._stationary_last_point
            .as_ref()
            .zip(self._stationary_last_time_s)?;
        let dt = now - time;
        if !(0. < dt && dt <= STATIONARY_MEASUREMENT_DROPOUT_HOLD_S) {
            return None;
        }
        let predicted = Point {
            d_rel: previous.d_rel + previous.v_rel * dt,
            y_rel: previous.y_rel + previous.yv_rel * dt,
            measured: false,
            ..previous.clone()
        };
        let strong = vision.is_some_and(|vision| vision.probability >= STATIONARY_VISION_MIN_PROB);
        let vision_supported = strong && cross_position_cost(vision, &predicted).is_some();
        let corner_supported = !strong
            && (previous.source == "frontRadar" || previous.source == "scc")
            && points.iter().any(|point| {
                point.measured
                    && point.corner()
                    && (point.d_rel - predicted.d_rel).abs()
                        <= FRONT_KINEMATIC_HOLD_MAX_DREL_DELTA_M
                    && (point.y_rel - predicted.y_rel).abs()
                        <= FRONT_KINEMATIC_HOLD_MAX_YREL_DELTA_M
                    && (point.v_lead - predicted.v_lead).abs()
                        <= FRONT_KINEMATIC_HOLD_MAX_VLEAD_DELTA_MPS
            });
        if !vision_supported && !corner_supported {
            return None;
        }
        let d_path = path.project(predicted.d_rel, predicted.y_rel).d_path;
        if d_path.abs() > STATIONARY_HELD_MAX_DPATH_M {
            return None;
        }
        Some(VisionMatch {
            point: predicted,
            probability: vision.map_or(self._stationary_seed_probability, |vision| {
                vision.probability
            }),
            score: maximum(0., 1. - self._stationary_seed_score),
            d_path,
        })
    }
    pub fn pending_measurement_hold(
        &self,
        points: &[Point],
        path: &Path,
        now: f64,
        allowed: Option<&HashSet<String>>,
    ) -> Option<Candidate> {
        let identity = self._stationary_pending_identity.as_ref()?;
        let previous = self._stationary_last_point.as_ref()?;
        let time = self._stationary_last_time_s?;
        let support_time = self._stationary_pending_last_support_time_s?;
        if self._stationary_seed_probability < STATIONARY_VISION_MIN_PROB
            || !(0. < now - support_time
                && now - support_time <= STATIONARY_MEASUREMENT_DROPOUT_HOLD_S)
        {
            return None;
        }
        for point in points {
            if &point.identity() != identity
                || point.source != "frontRadar"
                || allowed.is_some_and(|allowed| !allowed.contains(&point.source))
                || !point.measured
                || point.radar_track_state < 2
                || !(0.5 < point.d_rel && point.d_rel < 180.)
                || point.v_lead.abs() > STATIONARY_MAX_ABS_VLEAD_MPS
                || !position_continuous(previous, time, point, now)
            {
                continue;
            }
            let d_path = path.project(point.d_rel, point.y_rel).d_path;
            if d_path.abs() <= STATIONARY_HELD_FRONT_NO_VISION_MAX_DPATH_M {
                return Some((point.clone(), d_path, self._stationary_seed_score));
            }
        }
        None
    }
    pub fn weak_pair_support(
        &mut self,
        vision: Option<VisionLead>,
        points: &[Point],
        path: &Path,
        now: f64,
    ) -> Vec<Candidate> {
        let current =
            vision.is_some_and(|vision| vision.probability >= STATIONARY_WEAK_VISION_MIN_PROB);
        let held = self._stationary_weak_pair_identity.is_some()
            && self
                ._stationary_weak_pair_last_vision_time_s
                .is_some_and(|time| now - time <= STATIONARY_WEAK_VISION_PAIR_HOLD_S);
        if !current && !held {
            self._stationary_weak_pair_identity = None;
            self._stationary_weak_pair_last_vision_time_s = None;
            return Vec::new();
        }
        let fronts: Vec<_> = points
            .iter()
            .filter(|point| {
                point.measured
                    && point.source == "frontRadar"
                    && 0.5 < point.d_rel
                    && point.d_rel < 180.
                    && point.v_lead.abs() <= STATIONARY_MAX_ABS_VLEAD_MPS
            })
            .map(|point| (point, path.project(point.d_rel, point.y_rel).d_path))
            .collect();
        let corners: Vec<_> = points
            .iter()
            .filter(|point| {
                point.measured
                    && point.corner()
                    && 0.5 < point.d_rel
                    && point.d_rel < 180.
                    && point.v_lead.abs() <= STATIONARY_VISION_CROSS_SOURCE_CORNER_MAX_ABS_VLEAD_MPS
            })
            .map(|point| (point, path.project(point.d_rel, point.y_rel).d_path))
            .collect();
        let mut pairs = Vec::new();
        for (front, front_path) in &fronts {
            for (corner, corner_path) in &corners {
                if (front.d_rel - corner.d_rel).abs() <= STATIONARY_WEAK_PAIR_MAX_DREL_M
                    && (front_path - corner_path).abs() <= STATIONARY_WEAK_PAIR_MAX_DPATH_DELTA_M
                    && (front.v_lead - corner.v_lead).abs()
                        <= STATIONARY_WEAK_PAIR_MAX_VLEAD_DELTA_MPS
                    && front_path.abs() <= STATIONARY_WEAK_PAIR_FRONT_MAX_DPATH_M
                    && corner_path.abs() <= STATIONARY_WEAK_PAIR_CORNER_MAX_DPATH_M
                {
                    pairs.push((*front, *front_path, *corner, *corner_path));
                }
            }
        }
        if pairs.is_empty() {
            self._stationary_weak_pair_identity = None;
            self._stationary_weak_pair_last_vision_time_s = None;
            return Vec::new();
        }
        let vision_path = vision.map_or(f64::INFINITY, |vision| {
            path.project(vision.d_rel, vision.y_rel).d_path
        });
        let supported: Vec<_> = pairs
            .iter()
            .filter(|(front, _, corner, _)| {
                vision.is_some_and(|vision| {
                    vision.probability >= STATIONARY_WEAK_VISION_MIN_PROB
                        && vision_path.abs() <= STATIONARY_WEAK_VISION_MAX_DPATH_M
                        && minimum(
                            (vision.d_rel - front.d_rel).abs(),
                            (vision.d_rel - corner.d_rel).abs(),
                        ) <= STATIONARY_WEAK_VISION_MAX_DISTANCE_ERROR_M
                        && minimum(
                            (vision.velocity - front.v_lead).abs(),
                            (vision.velocity - corner.v_lead).abs(),
                        ) <= STATIONARY_WEAK_VISION_MAX_SPEED_DELTA_MPS
                })
            })
            .collect();
        let selected = if let Some(pair) = supported.into_iter().min_by(|left, right| {
            let key = |(front, front_path, corner, corner_path): &&(&Point, f64, &Point, f64)| {
                (
                    front_path.abs() + corner_path.abs(),
                    (front.d_rel - corner.d_rel).abs(),
                    front.track_id,
                    corner.track_id,
                )
            };
            key(left)
                .partial_cmp(&key(right))
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            self._stationary_weak_pair_identity =
                Some((pair.0.track_id, pair.2.source.clone(), pair.2.track_id));
            self._stationary_weak_pair_last_vision_time_s = Some(now);
            Some(pair)
        } else if self._stationary_weak_pair_identity.is_some()
            && self
                ._stationary_weak_pair_last_vision_time_s
                .is_some_and(|time| {
                    let expired = now - time > STATIONARY_WEAK_VISION_PAIR_HOLD_S;
                    !expired
                })
        {
            pairs.iter().find(|pair| {
                self._stationary_weak_pair_identity.as_ref()
                    == Some(&(pair.0.track_id, pair.2.source.clone(), pair.2.track_id))
            })
        } else {
            None
        };
        let Some((front, front_path, corner, corner_path)) = selected else {
            self._stationary_weak_pair_identity = None;
            self._stationary_weak_pair_last_vision_time_s = None;
            return Vec::new();
        };
        let cost = (front.d_rel - corner.d_rel).abs() / STATIONARY_WEAK_PAIR_MAX_DREL_M;
        vec![
            (
                (*front).clone(),
                *front_path,
                cost + front_path.abs() / STATIONARY_WEAK_PAIR_FRONT_MAX_DPATH_M,
            ),
            (
                (*corner).clone(),
                *corner_path,
                cost + corner_path.abs() / STATIONARY_WEAK_PAIR_CORNER_MAX_DPATH_M,
            ),
        ]
    }
}
