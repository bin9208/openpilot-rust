use super::{
    constants::*, source::position_continuous, stationary_geometry::front_corner_pairs, Matcher,
    PositionEvidence,
};
use crate::{
    math::{maximum, minimum},
    model::VisionLead,
    path::Path,
    point::{Identity, Point},
};
use indexmap::IndexMap;
use std::collections::HashSet;

impl Matcher {
    pub fn update_front_evidence(
        &mut self,
        vision: Option<VisionLead>,
        points: &[Point],
        path: &Path,
        time: Option<f64>,
        yaw: f64,
    ) {
        let mut current = IndexMap::new();
        if let Some(now) = time.filter(|now| now.is_finite()) {
            for point in points {
                if point.source != "frontRadar"
                    || !point.measured
                    || point.radar_track_state < STATIONARY_FRONT_POSITION_LOCK_MIN_TRACK_STATE
                    || point.v_lead.abs() > STATIONARY_MAX_ABS_VLEAD_MPS
                {
                    continue;
                }
                let identity = point.identity();
                let previous = self._stationary_front_evidence.get(&identity);
                let continuous = previous.is_some_and(|previous| {
                    0. < now - previous.time_s
                        && now - previous.time_s <= VISION_CORROBORATED_MAX_OBSERVATION_GAP_S
                        && position_continuous(&previous.point, previous.time_s, point, now)
                        && (point.v_lead - previous.point.v_lead).abs()
                            <= STATIONARY_FRONT_ANCHOR_MAX_SPEED_JUMP_MPS
                });
                let since_s = previous
                    .filter(|_| continuous)
                    .map_or(now, |previous| previous.since_s);
                let mut evidence = PositionEvidence {
                    since_s,
                    time_s: now,
                    point: point.clone(),
                    anchor_frames: 0,
                    anchor_time_s: None,
                    position_since_s: None,
                    position_frames: 0,
                };
                if let Some(vision) = vision.filter(|vision| {
                    vision.probability >= STATIONARY_VISION_MIN_PROB
                        && point.d_rel >= STATIONARY_FRONT_POSITION_LOCK_MIN_DREL_M
                        && point.v_lead.abs() <= STATIONARY_FRONT_ANCHOR_MAX_ABS_VLEAD_MPS
                        && (point.d_rel - vision.d_rel).abs() <= STATIONARY_FRONT_RANGE_MAX_ERROR_M
                        && (point.y_rel - vision.y_rel).abs()
                            <= STATIONARY_FRONT_POSITION_LOCK_MAX_YREL_ERROR_M
                        && yaw.is_finite()
                        && yaw.abs() < STATIONARY_TURN_MIN_ABS_YAW_RATE_RAD_S
                        && path.project(point.d_rel, point.y_rel).d_path.abs()
                            <= STATIONARY_FRONT_ANCHOR_MAX_DPATH_M
                }) {
                    if let Some(previous) = previous.filter(|previous| {
                        continuous
                            && previous
                                .anchor_time_s
                                .is_some_and(|time| now - time <= STATIONARY_FRONT_ANCHOR_MAX_AGE_S)
                    }) {
                        evidence.anchor_frames = previous.anchor_frames;
                        evidence.anchor_time_s = previous.anchor_time_s;
                    }
                    if (point.d_rel - vision.d_rel).abs()
                        <= STATIONARY_FRONT_POSITION_LOCK_MAX_DISTANCE_ERROR_M
                    {
                        evidence.anchor_frames =
                            (evidence.anchor_frames + 1).min(STATIONARY_FRONT_ANCHOR_MIN_FRAMES);
                        evidence.anchor_time_s = Some(now);
                        if vision.probability >= STATIONARY_FRONT_POSITION_HISTORY_MIN_PROB {
                            let position_continuous = previous.is_some_and(|previous| {
                                continuous
                                    && previous.position_since_s.is_some()
                                    && (point.d_rel
                                        - previous.point.d_rel
                                        - 0.5
                                            * (point.v_rel + previous.point.v_rel)
                                            * (now - previous.time_s))
                                        .abs()
                                        <= STATIONARY_FRONT_POSITION_HISTORY_MAX_RANGE_RESIDUAL_M
                            });
                            evidence.position_since_s = Some(
                                previous
                                    .filter(|_| position_continuous)
                                    .and_then(|previous| previous.position_since_s)
                                    .unwrap_or(now),
                            );
                            evidence.position_frames = previous
                                .filter(|_| position_continuous)
                                .map_or(1, |previous| (previous.position_frames + 1).min(6));
                        }
                    }
                }
                current.insert(identity, evidence);
            }
        }
        self._stationary_front_evidence = current;
    }
    pub fn front_position_confirmed(
        &self,
        vision: Option<VisionLead>,
        point: &Point,
        now: f64,
    ) -> bool {
        vision.is_some_and(|vision| {
            vision.probability >= STATIONARY_FRONT_POSITION_HISTORY_CONFIRM_PROB
        }) && self
            ._stationary_front_evidence
            .get(&point.identity())
            .is_some_and(|evidence| {
                evidence.time_s == now
                    && evidence
                        .position_since_s
                        .is_some_and(|since| now - since >= STATIONARY_CONFIRMATION_S)
                    && evidence.position_frames >= STATIONARY_FRONT_MIN_VISION_SUPPORT_FRAMES
            })
    }
    pub fn front_anchor_time(&self, point: &Point, now: f64) -> Option<f64> {
        self._stationary_front_evidence
            .get(&point.identity())
            .filter(|evidence| {
                evidence.time_s == now
                    && evidence.anchor_frames >= STATIONARY_FRONT_ANCHOR_MIN_FRAMES
                    && now - evidence.since_s >= STATIONARY_FRONT_POSITION_LOCK_MIN_OBSERVED_S
            })
            .and_then(|evidence| evidence.anchor_time_s)
    }
    pub fn anchored_front_cost(
        &self,
        vision: Option<VisionLead>,
        point: &Point,
        d_path: f64,
        now: f64,
        yaw: f64,
    ) -> Option<f64> {
        let vision = vision?;
        if vision.probability < STATIONARY_VISION_MIN_PROB
            || (self.front_anchor_time(point, now).is_none()
                && !self.front_position_confirmed(Some(vision), point, now))
            || (point.v_lead - vision.velocity).abs()
                > STATIONARY_FRONT_ANCHOR_MAX_VISION_SPEED_DELTA_MPS
            || !vision.v_std.is_finite()
            || vision.v_std <= 0.
            || (point.v_lead - vision.velocity).abs()
                > STATIONARY_FRONT_ANCHOR_VSTD_SIGMA * vision.v_std
            || (point.d_rel - vision.d_rel).abs() > VISION_RADAR_MAX_DISTANCE_ERROR_M
            || d_path.abs() > STATIONARY_FRONT_ANCHOR_MAX_DPATH_M
            || !yaw.is_finite()
            || yaw.abs() >= STATIONARY_TURN_MIN_ABS_YAW_RATE_RAD_S
        {
            return None;
        }
        Some(
            (point.d_rel - vision.d_rel).abs() / VISION_RADAR_MAX_DISTANCE_ERROR_M
                + (point.y_rel - vision.y_rel).abs()
                    / STATIONARY_FRONT_POSITION_LOCK_MAX_YREL_ERROR_M
                + 0.15 * d_path.abs() / STATIONARY_FRONT_ANCHOR_MAX_DPATH_M,
        )
    }
    pub fn held_front_range_limit(
        &self,
        vision: VisionLead,
        point: &Point,
        path: &Path,
        now: f64,
    ) -> f64 {
        if self.stationary_identity.as_ref() == Some(&point.identity())
            && point.source == "frontRadar"
            && point.measured
            && point.radar_track_state >= STATIONARY_FRONT_POSITION_LOCK_MIN_TRACK_STATE
            && point.v_lead.abs() <= STATIONARY_MAX_ABS_VLEAD_MPS
            && (point.y_rel - vision.y_rel).abs() <= STATIONARY_FRONT_RANGE_MAX_YREL_ERROR_M
            && self
                ._stationary_last_point
                .as_ref()
                .zip(self._stationary_last_time_s)
                .is_some_and(|(previous, time)| {
                    0. < now - time
                        && now - time <= VISION_CORROBORATED_MAX_OBSERVATION_GAP_S
                        && position_continuous(previous, time, point, now)
                })
            && vision.x_std.is_finite()
            && vision.x_std > 0.
            && path.project(point.d_rel, point.y_rel).d_path.abs()
                <= STATIONARY_FRONT_RANGE_MAX_DPATH_M
        {
            maximum(
                VISION_RADAR_MAX_DISTANCE_ERROR_M,
                minimum(
                    minimum(
                        STATIONARY_FRONT_RANGE_MAX_ERROR_M,
                        STATIONARY_FRONT_RANGE_XSTD_SIGMA * vision.x_std,
                    ),
                    STATIONARY_FRONT_RANGE_MAX_FRACTION * vision.d_rel,
                ),
            )
        } else {
            VISION_RADAR_MAX_DISTANCE_ERROR_M
        }
    }
    pub fn front_position_lock_cost(
        &self,
        vision: Option<VisionLead>,
        point: &Point,
        d_path: f64,
        now: f64,
        yaw: f64,
    ) -> Option<f64> {
        if let Some(cost) = self.anchored_front_cost(vision, point, d_path, now, yaw) {
            return Some(cost);
        }
        let vision = vision?;
        let since = self._observed_since_s.get(&point.identity()).copied();
        if vision.probability < STATIONARY_VISION_MIN_PROB
            || point.source != "frontRadar"
            || !point.measured
            || point.radar_track_state < STATIONARY_FRONT_POSITION_LOCK_MIN_TRACK_STATE
            || point.d_rel < STATIONARY_FRONT_POSITION_LOCK_MIN_DREL_M
            || point.v_lead.abs() > STATIONARY_FRONT_POSITION_LOCK_MAX_ABS_VLEAD_MPS
            || (point.v_lead - vision.velocity).abs()
                > STATIONARY_FRONT_POSITION_LOCK_MAX_VISION_SPEED_DELTA_MPS
            || (point.d_rel - vision.d_rel).abs()
                > STATIONARY_FRONT_POSITION_LOCK_MAX_DISTANCE_ERROR_M
            || (point.y_rel - vision.y_rel).abs() > STATIONARY_FRONT_POSITION_LOCK_MAX_YREL_ERROR_M
            || d_path.abs() > STATIONARY_FRONT_POSITION_LOCK_MAX_DPATH_M
            || yaw.abs() >= STATIONARY_FRONT_POSITION_LOCK_MAX_ABS_YAW_RATE_RAD_S
            || since.is_none_or(|since| now - since < STATIONARY_FRONT_POSITION_LOCK_MIN_OBSERVED_S)
        {
            return None;
        }
        Some(
            (point.d_rel - vision.d_rel).abs()
                / STATIONARY_FRONT_POSITION_LOCK_MAX_DISTANCE_ERROR_M
                + (point.y_rel - vision.y_rel).abs()
                    / STATIONARY_FRONT_POSITION_LOCK_MAX_YREL_ERROR_M
                + 0.15 * d_path.abs() / STATIONARY_FRONT_POSITION_LOCK_MAX_DPATH_M,
        )
    }
    pub fn moving_vision_conflicts(
        &mut self,
        vision: Option<VisionLead>,
        points: &[Point],
        path: &Path,
        time: Option<f64>,
        yaw: f64,
    ) -> HashSet<Identity> {
        let Some((vision, now)) = vision
            .filter(|vision| vision.probability >= STATIONARY_VISION_MIN_PROB)
            .zip(time.filter(|time| time.is_finite()))
        else {
            self._moving_vision_evidence.clear();
            return HashSet::new();
        };
        let low_speed: HashSet<_> = points
            .iter()
            .filter(|point| {
                point.source == "frontRadar"
                    && point.v_lead.abs() <= STATIONARY_MAX_ABS_VLEAD_MPS
                    && point.d_rel < STATIONARY_FRONT_POSITION_LOCK_MIN_DREL_M
                    && vision.probability >= VISION_RADAR_FAR_MIN_SEED_PROB
                    && (point.v_lead - vision.velocity).abs()
                        > maximum(
                            STATIONARY_MOVING_VISION_MAX_SPEED_ERROR_MPS,
                            3. * vision.v_std.abs(),
                        )
            })
            .map(Point::identity)
            .collect();
        let fronts: Vec<_> = points
            .iter()
            .filter(|point| {
                point.source == "frontRadar"
                    && point.v_lead.abs() <= STATIONARY_MAX_ABS_VLEAD_MPS
                    && ((point.v_lead - vision.velocity).abs()
                        > STATIONARY_MOVING_VISION_MIN_SPEED_DELTA_MPS
                        || low_speed.contains(&point.identity()))
            })
            .collect();
        if fronts.is_empty() {
            self._moving_vision_evidence.clear();
            return HashSet::new();
        }
        let independent: HashSet<_> = front_corner_pairs(points, path)
            .into_iter()
            .map(|(front, _, _, _)| front.identity())
            .collect();
        let mut conflicts: HashSet<_> = fronts
            .iter()
            .filter(|point| {
                point.d_rel < STATIONARY_FRONT_POSITION_LOCK_MIN_DREL_M
                    && vision.probability >= VISION_RADAR_FAR_MIN_SEED_PROB
                    && (point.v_lead - vision.velocity).abs()
                        > maximum(
                            STATIONARY_MOVING_VISION_MIN_SPEED_DELTA_MPS,
                            minimum(
                                STATIONARY_MAX_VISION_SPEED_DELTA_MPS,
                                3. * vision.v_std.abs(),
                            ),
                        )
                    && !independent.contains(&point.identity())
            })
            .map(|point| point.identity())
            .collect();
        let support: Vec<_> = points
            .iter()
            .filter(|point| {
                point.measured
                    && 0.5 < point.d_rel
                    && point.d_rel < 180.
                    && (point.source == "frontRadar" || point.source == "scc" || point.corner())
                    && point.v_lead > STATIONARY_MAX_ABS_VLEAD_MPS
                    && (point.v_lead - vision.velocity).abs()
                        <= STATIONARY_MOVING_VISION_MAX_SPEED_ERROR_MPS
                    && (point.d_rel - vision.d_rel).abs() <= VISION_RADAR_MAX_DISTANCE_ERROR_M
                    && (point.y_rel - vision.y_rel).abs()
                        <= STATIONARY_MOVING_VISION_MAX_YREL_ERROR_M
                    && now
                        - self
                            ._observed_since_s
                            .get(&point.identity())
                            .copied()
                            .unwrap_or(now)
                        >= VISION_CORROBORATED_MIN_OBSERVED_S
                    && path.project(point.d_rel, point.y_rel).d_path.abs()
                        <= VISION_MATCH_FRESH_MAX_DPATH_M
            })
            .collect();
        let mut evidence = IndexMap::new();
        for point in &support {
            let identity = point.identity();
            let previous = self._moving_vision_evidence.get(&identity);
            let since_s = previous
                .filter(|previous| {
                    let dt = now - previous.time_s;
                    0. < dt
                        && dt <= STATIONARY_MEASUREMENT_DROPOUT_HOLD_S
                        && (point.d_rel - (previous.point.d_rel + previous.point.v_rel * dt)).abs()
                            <= STATIONARY_LONGITUDINAL_CONTINUITY_M
                        && (point.y_rel - (previous.point.y_rel + previous.point.yv_rel * dt)).abs()
                            <= STATIONARY_VISION_CROSS_SOURCE_MAX_YREL_M
                        && (point.v_lead - previous.point.v_lead).abs() <= 3.
                })
                .map_or(now, |previous| previous.since_s);
            evidence.insert(
                identity,
                PositionEvidence {
                    since_s,
                    time_s: now,
                    point: (*point).clone(),
                    anchor_frames: 0,
                    anchor_time_s: None,
                    position_since_s: None,
                    position_frames: 0,
                },
            );
        }
        self._moving_vision_evidence = evidence;
        if support.is_empty() {
            return conflicts;
        }
        let continuous_support = self._moving_vision_evidence.values().any(|item| {
            item.point.source == "frontRadar"
                || item.point.source == "scc"
                || now - item.since_s >= STATIONARY_MOVING_CORNER_CONFIRMATION_S
        });
        for point in fronts {
            let identity = point.identity();
            if !independent.contains(&identity)
                && ((point.v_lead - vision.velocity).abs()
                    > STATIONARY_MOVING_VISION_MIN_SPEED_DELTA_MPS
                    || (low_speed.contains(&identity) && continuous_support))
                && (continuous_support
                    || self
                        .anchored_front_cost(
                            Some(vision),
                            point,
                            path.project(point.d_rel, point.y_rel).d_path,
                            now,
                            yaw,
                        )
                        .is_none())
            {
                conflicts.insert(identity);
            }
        }
        conflicts
    }
}
