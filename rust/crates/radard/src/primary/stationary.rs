use super::{
    constants::*,
    source::{position_continuous, stationary_source_rank},
    stationary_geometry::{
        corroborating_fronts, cross_front_support, cross_position_cost, front_corner_pairs,
        vision_base_cost, vision_cost,
    },
    stationary_support::{radar_only_support, vision_path_compatible, Candidate},
    Frame, Matcher, VisionMatch,
};
use crate::{
    math::{maximum, minimum},
    point::Point,
};
use indexmap::IndexMap;
use std::collections::HashSet;

fn choose(
    candidates: &[Candidate],
    key: impl Fn(&Candidate) -> (f64, crate::point::TrackId),
) -> Option<Candidate> {
    candidates
        .iter()
        .min_by(|left, right| {
            key(left)
                .partial_cmp(&key(right))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned()
}

impl Matcher {
    fn project_stationary(
        &self,
        point: &Point,
        frame: &Frame<'_>,
        now: f64,
        strong: bool,
    ) -> Option<(Point, f64)> {
        let identity = point.identity();
        let strong_support = strong && vision_base_cost(frame.vision, point).is_some();
        let d_path = frame.path.project(point.d_rel, point.y_rel).d_path;
        let mut limit = STATIONARY_FRESH_MAX_DPATH_M;
        if self.stationary_identity.as_ref() == Some(&identity) {
            if point.source == "frontRadar" && !strong_support && !self._stationary_corner_supported
            {
                limit = if point.d_rel >= STATIONARY_RADAR_ONLY_FRONT_FAR_DREL_M {
                    STATIONARY_RADAR_ONLY_FRONT_FAR_MAX_DPATH_M
                } else {
                    STATIONARY_RADAR_ONLY_HELD_MAX_DPATH_M
                };
            } else {
                limit = if self._stationary_seed_probability >= STATIONARY_VISION_MIN_PROB
                    || strong_support
                {
                    STATIONARY_HELD_MAX_DPATH_M
                } else {
                    STATIONARY_RADAR_ONLY_HELD_MAX_DPATH_M
                };
            }
        }
        let outlier = self.stationary_identity.as_ref() == Some(&identity)
            && (self._stationary_seed_probability >= STATIONARY_VISION_MIN_PROB || strong_support)
            && frame.vision.is_some_and(|vision| {
                vision.probability >= STATIONARY_VISION_PATH_OUTLIER_MIN_PROB
            })
            && d_path.abs() <= STATIONARY_VISION_PATH_OUTLIER_MAX_DPATH_M
            && self
                ._stationary_path_outlier_since_s
                .is_none_or(|since| now - since <= STATIONARY_VISION_PATH_OUTLIER_HOLD_S);
        (d_path.abs() <= limit || outlier).then(|| (point.clone(), d_path))
    }

    pub fn match_stationary(&mut self, frame: &Frame<'_>) -> Option<VisionMatch> {
        let Some(now) = frame.time.filter(|now| now.is_finite()) else {
            self.reset_stationary();
            return None;
        };
        let strong = frame
            .vision
            .is_some_and(|vision| vision.probability >= STATIONARY_VISION_MIN_PROB);
        let vision_path = frame
            .vision
            .filter(|_| strong)
            .map(|vision| frame.path.project(vision.d_rel, vision.y_rel).d_path);
        let need_pairs = strong
            || (self._stationary_corner_supported
                && (self.stationary_identity.is_some()
                    || self._stationary_pending_identity.is_some()));
        let pairs = if need_pairs {
            front_corner_pairs(frame.points, frame.path)
        } else {
            Vec::new()
        };
        let paired: HashSet<_> = pairs
            .iter()
            .map(|(front, _, _, _)| front.identity())
            .collect();
        let cross_support: IndexMap<_, _> = cross_front_support(frame.vision, &pairs)
            .into_iter()
            .map(|candidate| (candidate.0.identity(), candidate))
            .collect();
        let mut held_cost = IndexMap::new();
        let mut eligible = Vec::new();
        let held_present = frame
            .points
            .iter()
            .any(|point| self.stationary_identity.as_ref() == Some(&point.identity()));
        for point in frame.points {
            let identity = point.identity();
            let held = self.stationary_identity.as_ref() == Some(&identity);
            let pending = self._stationary_pending_identity.as_ref() == Some(&identity);
            let continuous_corner = strong
                && self.stationary_identity.is_some()
                && !held_present
                && self.corner_slot_continuous(point, now);
            let held_paired_front = strong
                && self._stationary_corner_supported
                && held
                && point.source == "frontRadar"
                && point.v_lead.abs() <= STATIONARY_MAX_ABS_VLEAD_MPS;
            if (strong
                && (held || pending)
                && point.corner()
                && point.v_lead.abs() <= STATIONARY_HELD_CORNER_MAX_ABS_VLEAD_MPS)
                || continuous_corner
                || held_paired_front
            {
                if let Some(cost) = cross_position_cost(frame.vision, point) {
                    held_cost.insert(identity.clone(), cost);
                }
            }
            let retained_pair = paired.contains(&identity)
                && self._stationary_corner_supported
                && (held || pending)
                && (!strong || cross_position_cost(frame.vision, point).is_some());
            let held_closer_front = strong
                && frame.vision.is_some_and(|vision| {
                    held && point.source == "frontRadar"
                        && point.measured
                        && point.radar_track_state >= STATIONARY_RADAR_ONLY_FRONT_MIN_TRACK_STATE
                        && point.d_rel < vision.d_rel
                        && point.v_lead.abs() <= STATIONARY_MAX_ABS_VLEAD_MPS
                        && (point.v_lead - vision.velocity).abs()
                            <= STATIONARY_MAX_VISION_SPEED_DELTA_MPS
                        && frame.path.project(point.d_rel, point.y_rel).d_path.abs()
                            <= if point.d_rel >= STATIONARY_RADAR_ONLY_FRONT_FAR_DREL_M {
                                STATIONARY_RADAR_ONLY_FRONT_FAR_MAX_DPATH_M
                            } else {
                                STATIONARY_RADAR_ONLY_HELD_MAX_DPATH_M
                            }
                        && frame.yaw_rate.abs() < STATIONARY_RADAR_ONLY_FRONT_MAX_ABS_YAW_RATE_RAD_S
                });
            if !(0.5 < point.d_rel && point.d_rel < 180.)
                || (point.v_lead.abs() > STATIONARY_MAX_ABS_VLEAD_MPS
                    && !held_cost.contains_key(&identity))
            {
                continue;
            }
            if strong
                && frame.vision.is_some_and(|vision| {
                    (point.d_rel - vision.d_rel).abs()
                        > self.held_front_range_limit(vision, point, frame.path, now)
                })
                && !held_cost.contains_key(&identity)
                && !cross_support.contains_key(&identity)
                && !retained_pair
                && !held_closer_front
            {
                continue;
            }
            eligible.push(point.clone());
        }
        let projection_points = if strong {
            eligible.clone()
        } else {
            self.initial_projection_points(&eligible, now)
        };
        let mut candidates: Vec<_> = projection_points
            .iter()
            .filter_map(|point| self.project_stationary(point, frame, now, strong))
            .collect();
        if !strong {
            for point in corroborating_fronts(&eligible, &candidates) {
                if let Some(candidate) = self.project_stationary(&point, frame, now, strong) {
                    candidates.push(candidate);
                }
            }
        }
        if frame.prefer_corner && candidates.iter().any(|(point, _)| point.corner()) {
            candidates.retain(|(point, _)| point.corner());
        }
        let mut supported = Vec::new();
        let mut weak_supported = Vec::new();
        if let Some(vision) = frame.vision.filter(|_| strong) {
            for (point, d_path) in &candidates {
                let identity = point.identity();
                if vision_path.is_none_or(|vision_path| {
                    !vision_path_compatible(
                        vision_path,
                        *d_path,
                        self.stationary_identity.as_ref() == Some(&identity),
                    )
                }) {
                    continue;
                }
                if point.source == "frontRadar"
                    && !cross_support.contains_key(&identity)
                    && frame.yaw_rate.abs() >= STATIONARY_TURN_FRONT_MIN_ABS_YAW_RATE_RAD_S
                    && (point.v_lead - vision.velocity).abs()
                        > STATIONARY_TURN_FRONT_FAST_VISION_SPEED_DELTA_MPS
                    && (point.y_rel - vision.y_rel).abs()
                        > STATIONARY_TURN_FRONT_FAST_VISION_MAX_YREL_ERROR_M
                {
                    continue;
                }
                let cost = vision_cost(vision, point, *d_path, frame.prefer_corner)
                    .or_else(|| {
                        self.front_position_lock_cost(
                            Some(vision),
                            point,
                            *d_path,
                            now,
                            frame.yaw_rate,
                        )
                    })
                    .or_else(|| {
                        held_cost
                            .get(&identity)
                            .map(|cost| cost + 0.15 * d_path.abs() / STATIONARY_HELD_MAX_DPATH_M)
                            .or_else(|| cross_support.get(&identity).map(|candidate| candidate.2))
                    });
                if let Some(cost) = cost {
                    supported.push((point.clone(), *d_path, cost));
                }
            }
        } else {
            supported = radar_only_support(&candidates, frame.yaw_rate);
            weak_supported = self.weak_pair_support(frame.vision, frame.points, frame.path, now);
            if !weak_supported.is_empty() {
                supported.clone_from(&weak_supported);
            }
            if !frame.prefer_primary
                && weak_supported.is_empty()
                && supported.iter().any(|(point, _, _)| point.corner())
            {
                supported.retain(|(point, _, _)| point.corner());
            }
        }
        let corner_supported: HashSet<_> = supported
            .iter()
            .filter(|(point, _, _)| {
                point.corner()
                    || (point.source == "frontRadar"
                        && (!strong || paired.contains(&point.identity())))
            })
            .map(|(point, _, _)| point.identity())
            .collect();
        if let Some(allowed) = frame.allowed_output_sources {
            candidates.retain(|(point, _)| allowed.contains(&point.source));
            supported.retain(|(point, _, _)| allowed.contains(&point.source));
        }
        if frame.prefer_primary {
            if let Some(rank) = supported
                .iter()
                .map(|(point, _, _)| stationary_source_rank(point))
                .min()
            {
                supported.retain(|(point, _, _)| stationary_source_rank(point) == rank);
            }
        }
        let selected = if let Some(held) = self.stationary_identity.as_ref() {
            if corner_supported.contains(held) {
                self._stationary_corner_supported = true;
            }
            let with_hold: Vec<_> = supported
                .iter()
                .map(|(point, d_path, cost)| {
                    (
                        point.clone(),
                        *d_path,
                        cost - if &point.identity() == held { 0.75 } else { 0. },
                    )
                })
                .collect();
            let selected = if !with_hold.is_empty() {
                choose(&with_hold, |candidate| (candidate.2, candidate.0.track_id))
            } else if let Some((previous, time)) = self
                ._stationary_last_point
                .as_ref()
                .zip(self._stationary_last_time_s)
            {
                candidates
                    .iter()
                    .filter(|(point, _)| {
                        (&point.identity() == held || self.cross_source_continuous(point, now))
                            && position_continuous(previous, time, point, now)
                    })
                    .min_by(|(left, _), (right, _)| {
                        (
                            stationary_source_rank(left),
                            (left.d_rel - previous.d_rel).abs(),
                        )
                            .partial_cmp(&(
                                stationary_source_rank(right),
                                (right.d_rel - previous.d_rel).abs(),
                            ))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(point, d_path)| (point.clone(), *d_path, self._stationary_seed_score))
            } else {
                None
            };
            let Some(selected) = selected else {
                if let Some(hold) = self.dropout_hold(frame.vision, frame.points, frame.path, now) {
                    return Some(hold);
                }
                self.reset_stationary();
                return None;
            };
            selected
        } else {
            let pending_hold = self.pending_measurement_hold(
                frame.points,
                frame.path,
                now,
                frame.allowed_output_sources,
            );
            let selected = if !supported.is_empty() {
                choose(&supported, |candidate| {
                    (
                        candidate.2
                            - if self._stationary_pending_identity.as_ref()
                                == Some(&candidate.0.identity())
                            {
                                0.75
                            } else {
                                0.
                            },
                        candidate.0.track_id,
                    )
                })
            } else if let Some((previous, time)) = self
                ._stationary_last_point
                .as_ref()
                .zip(self._stationary_last_time_s)
                .filter(|_| self._stationary_pending_identity.is_some())
            {
                candidates
                    .iter()
                    .find(|(point, _)| {
                        self._stationary_pending_identity.as_ref() == Some(&point.identity())
                            && position_continuous(previous, time, point, now)
                    })
                    .map(|(point, d_path)| (point.clone(), *d_path, self._stationary_seed_score))
            } else {
                None
            };
            let Some(selected) = selected.or_else(|| pending_hold.clone()) else {
                self.reset_stationary();
                return None;
            };
            let identity = selected.0.identity();
            let corner = corner_supported.contains(&identity);
            let current_support = supported
                .iter()
                .any(|(point, _, _)| point.identity() == identity);
            let pending_held = !current_support
                && pending_hold
                    .as_ref()
                    .is_some_and(|candidate| candidate.0.identity() == identity);
            if self._stationary_pending_identity.as_ref() == Some(&identity)
                && current_support
                && self._stationary_seed_probability < STATIONARY_VISION_MIN_PROB
                && !self._stationary_pending_weak_pair_supported
                && self
                    ._stationary_last_point
                    .as_ref()
                    .zip(self._stationary_last_time_s)
                    .is_some_and(|(previous, time)| {
                        !position_continuous(previous, time, &selected.0, now)
                    })
            {
                self.reset_stationary();
                return None;
            }
            if self._stationary_pending_identity.as_ref() != Some(&identity) {
                let carry = self._stationary_seed_probability >= STATIONARY_VISION_MIN_PROB
                    && self.cross_source_continuous(&selected.0, now);
                self._stationary_pending_identity = Some(identity.clone());
                if !carry {
                    self._stationary_pending_since_s = Some(now);
                    self._stationary_pending_vision_support_frames = 0;
                    self._stationary_pending_last_support_time_s = None;
                }
                if current_support {
                    self._stationary_pending_vision_support_frames += 1;
                    self._stationary_pending_last_support_time_s = Some(now);
                }
                let probability = frame.vision.map_or(0., |vision| vision.probability);
                self._stationary_seed_probability = if carry {
                    maximum(self._stationary_seed_probability, probability)
                } else {
                    probability
                };
                self._stationary_seed_score = selected.2;
                self._stationary_corner_supported = if carry {
                    self._stationary_corner_supported || corner
                } else {
                    corner
                };
                self._stationary_pending_weak_pair_supported = weak_supported
                    .iter()
                    .any(|(point, _, _)| point.identity() == identity);
            } else if current_support {
                self._stationary_pending_vision_support_frames += 1;
                self._stationary_pending_last_support_time_s = Some(now);
                if let Some(vision) = frame.vision.filter(|_| strong) {
                    self._stationary_seed_probability =
                        maximum(self._stationary_seed_probability, vision.probability);
                }
                self._stationary_corner_supported |= corner;
                if self._stationary_seed_probability < STATIONARY_VISION_MIN_PROB {
                    self._stationary_pending_weak_pair_supported = !weak_supported.is_empty();
                }
            } else if self._stationary_seed_probability < STATIONARY_VISION_MIN_PROB
                || (!selected.0.corner() && !self._stationary_corner_supported && !pending_held)
            {
                self.reset_stationary();
                return None;
            }
            self._stationary_last_point = Some(selected.0.clone());
            self._stationary_last_time_s = Some(now);
            let required = if selected.0.corner() || self._stationary_corner_supported {
                1
            } else {
                STATIONARY_FRONT_MIN_VISION_SUPPORT_FRAMES
            };
            let position_confirmed = self.front_position_confirmed(frame.vision, &selected.0, now)
                && self
                    .anchored_front_cost(frame.vision, &selected.0, selected.1, now, frame.yaw_rate)
                    .is_some();
            let duration = if self._stationary_seed_probability >= STATIONARY_VISION_MIN_PROB {
                STATIONARY_CONFIRMATION_S
            } else if self._stationary_pending_weak_pair_supported {
                STATIONARY_WEAK_VISION_PAIR_CONFIRMATION_S
            } else {
                STATIONARY_RADAR_ONLY_CONFIRMATION_S
            };
            if !position_confirmed
                && (self
                    ._stationary_pending_since_s
                    .is_none_or(|since| now - since < duration)
                    || self._stationary_pending_vision_support_frames < required)
            {
                return None;
            }
            self.stationary_identity = Some(identity);
            selected
        };
        let (point, d_path, score) = selected;
        let identity = point.identity();
        if self.stationary_identity.as_ref() != Some(&identity) {
            self._stationary_corner_supported = corner_supported.contains(&identity)
                || (self._stationary_corner_supported && self.cross_source_continuous(&point, now));
        }
        let current_vision_support = strong
            && supported
                .iter()
                .any(|(point, _, _)| point.identity() == identity);
        if self.stationary_identity.as_ref() == Some(&identity)
            && point.source == "frontRadar"
            && !current_vision_support
            && d_path.abs() > STATIONARY_HELD_FRONT_DEPARTURE_DPATH_M
        {
            if let Some(since) = self._stationary_front_departure_since_s {
                if now - since >= STATIONARY_HELD_FRONT_DEPARTURE_CONFIRMATION_S {
                    self.reset_stationary();
                    return None;
                }
            } else {
                self._stationary_front_departure_since_s = Some(now);
            }
        } else {
            self._stationary_front_departure_since_s = None;
        }
        if let Some(vision) = frame.vision.filter(|_| current_vision_support) {
            self._stationary_seed_probability =
                maximum(self._stationary_seed_probability, vision.probability);
        }
        if d_path.abs() > STATIONARY_HELD_MAX_DPATH_M {
            self._stationary_path_outlier_since_s.get_or_insert(now);
        } else {
            self._stationary_path_outlier_since_s = None;
        }
        self.stationary_identity = Some(identity);
        self._stationary_last_point = Some(point.clone());
        self._stationary_last_time_s = Some(now);
        Some(VisionMatch {
            point,
            probability: frame
                .vision
                .map_or(self._stationary_seed_probability, |vision| {
                    vision.probability
                }),
            score: minimum(1., maximum(0., 1. - score)),
            d_path,
        })
    }
}
