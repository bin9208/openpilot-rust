use super::{constants::*, Controller};
use crate::{
    math::{maximum, minimum},
    model::VisionLead,
    path::Path,
    point::Point,
    primary::{constants::*, source::position_continuous, VisionMatch},
    Lead,
};

pub fn central_vision(vision: Option<VisionLead>, path: &Path) -> bool {
    vision.is_some_and(|vision| {
        vision.probability >= RADAR_VISION_FALLBACK_MIN_PROBABILITY
            && path.project(vision.d_rel, vision.y_rel).d_path.abs()
                <= RADAR_VISION_FALLBACK_MAX_ABS_DPATH_M
    })
}
impl<P: crate::predictor::CutOutPredictor> Controller<P> {
    pub(super) fn reset_stationary_range(&mut self) {
        self._stationary_range_last_point = None;
        self._stationary_range_last_time_s = None;
        self._stationary_range_anchor_time_s = None;
    }
    pub(super) fn reset_range_mismatch(&mut self) {
        self._stationary_vision_range_mismatch_identity = None;
        self._stationary_vision_range_mismatch_since_s = None;
    }
    fn stationary_front_limit(
        &self,
        matched: Option<&VisionMatch>,
        vision: Option<VisionLead>,
        time: f64,
    ) -> (f64, Option<f64>) {
        let mut limit = RADAR_MATCH_MAX_FARTHER_THAN_VISION_M;
        let Some((matched, vision)) = matched.zip(vision).filter(|_| time.is_finite()) else {
            return (limit, None);
        };
        let point = &matched.point;
        if point.source != "frontRadar"
            || !point.measured
            || point.radar_track_state < STATIONARY_FRONT_POSITION_LOCK_MIN_TRACK_STATE
            || point.v_lead.abs() > STATIONARY_MAX_ABS_VLEAD_MPS
            || self.primary_matcher.stationary_identity.as_ref() != Some(&point.identity())
            || matched.d_path.abs() > STATIONARY_FRONT_RANGE_MAX_DPATH_M
            || vision.probability < RADAR_VISION_FALLBACK_MIN_PROBABILITY
            || (point.y_rel - vision.y_rel).abs() > STATIONARY_FRONT_RANGE_MAX_YREL_ERROR_M
        {
            return (limit, None);
        }
        let mut anchor = None;
        if (point.d_rel - vision.d_rel).abs() <= limit {
            anchor = Some(time);
        } else if let Some(((last, last_time), anchor_time)) = self
            ._stationary_range_last_point
            .as_ref()
            .zip(self._stationary_range_last_time_s)
            .zip(self._stationary_range_anchor_time_s)
        {
            if 0. < time - last_time
                && time - last_time <= RADAR_MOTION_MAX_TIME_SKEW_S
                && time - anchor_time <= STATIONARY_FRONT_ANCHOR_MAX_AGE_S
                && position_continuous(last, last_time, point, time)
                && (point.v_lead - last.v_lead).abs() <= STATIONARY_FRONT_ANCHOR_MAX_SPEED_JUMP_MPS
            {
                anchor = Some(anchor_time);
            }
        }
        if anchor.is_none() {
            anchor = self.primary_matcher.front_anchor_time(point, time);
        }
        if anchor.is_some() && vision.x_std.is_finite() && vision.x_std > 0. {
            limit = maximum(
                limit,
                minimum(
                    minimum(
                        STATIONARY_FRONT_RANGE_MAX_ERROR_M,
                        STATIONARY_FRONT_RANGE_MAX_FRACTION * vision.d_rel,
                    ),
                    STATIONARY_FRONT_RANGE_XSTD_SIGMA * vision.x_std,
                ),
            );
        }
        (limit, anchor)
    }
    pub(super) fn reject_farther(
        &mut self,
        matched: Option<&VisionMatch>,
        vision: Option<VisionLead>,
        path: &Path,
        time: f64,
    ) -> bool {
        if let Some(matched) = matched.filter(|m| {
            self.primary_matcher
                .closing_range_match
                .as_ref()
                .is_some_and(|v| v.point.identity() == m.point.identity())
        }) {
            self._moving_range_last_point = Some(matched.point.clone());
            self._moving_range_last_time_s = Some(time);
            self.reset_range_mismatch();
            return false;
        }
        let moving = matched.is_some_and(|matched| {
            let point = &matched.point;
            point.source == "frontRadar"
                && point.measured
                && point.v_lead > STATIONARY_MAX_ABS_VLEAD_MPS
                && central_vision(vision, path)
                && vision.is_some_and(|vision| vision.velocity > STATIONARY_MAX_ABS_VLEAD_MPS)
                && matched.d_path.abs() <= RADAR_VISION_FALLBACK_MAX_ABS_DPATH_M
        });
        let mut range_limit = RADAR_MATCH_MAX_FARTHER_THAN_VISION_M;
        if moving {
            if let Some(((matched, vision), (previous, previous_time))) = matched.zip(vision).zip(
                self._moving_range_last_point
                    .as_ref()
                    .zip(self._moving_range_last_time_s),
            ) {
                let point = &matched.point;
                let dt = time - previous_time;
                if point.identity() == previous.identity()
                    && 0. < dt
                    && dt <= MOVING_FRONT_RANGE_MAX_GAP_S
                    && (point.d_rel - (previous.d_rel + previous.v_rel * dt)).abs()
                        <= MOVING_FRONT_RANGE_MAX_POSITION_ERROR_M
                    && (point.y_rel - (previous.y_rel + previous.yv_rel * dt)).abs()
                        <= MOVING_FRONT_RANGE_MAX_LATERAL_ERROR_M
                    && (point.v_lead - previous.v_lead).abs()
                        <= MOVING_FRONT_RANGE_MAX_SPEED_JUMP_MPS
                    && vision.x_std.is_finite()
                    && vision.x_std > 0.
                {
                    range_limit = maximum(
                        range_limit,
                        minimum(
                            minimum(
                                VISION_RADAR_MAX_DISTANCE_ERROR_M,
                                MOVING_FRONT_RANGE_MAX_DISTANCE_FRACTION * vision.d_rel,
                            ),
                            MOVING_FRONT_RANGE_XSTD_SIGMA * vision.x_std,
                        ),
                    );
                }
            }
        }
        let (stationary_limit, anchor) = self.stationary_front_limit(matched, vision, time);
        range_limit = maximum(range_limit, stationary_limit);
        self.reset_stationary_range();
        self._moving_range_last_point = None;
        self._moving_range_last_time_s = None;
        let farther = matched.is_some_and(|matched| {
            central_vision(vision, path)
                && vision.is_some_and(|vision| matched.point.d_rel - vision.d_rel > range_limit)
        });
        if !farther {
            if moving {
                self._moving_range_last_point = matched.map(|matched| matched.point.clone());
                self._moving_range_last_time_s = Some(time);
            }
            if anchor.is_some() {
                self._stationary_range_last_point = matched.map(|matched| matched.point.clone());
                self._stationary_range_last_time_s = Some(time);
                self._stationary_range_anchor_time_s = anchor;
            }
            self.reset_range_mismatch();
            return false;
        }
        let Some(matched) = matched else {
            return false;
        };
        let identity = matched.point.identity();
        let corroborated = self.primary_matcher.stationary_identity.as_ref() == Some(&identity)
            && self.primary_matcher._stationary_corner_supported
            && matched.point.source == "frontRadar"
            && matched.point.measured
            && matched.point.v_lead.abs() <= STATIONARY_MAX_ABS_VLEAD_MPS;
        if !corroborated {
            self.reset_range_mismatch();
            return true;
        }
        if self._stationary_vision_range_mismatch_identity.as_ref() != Some(&identity) {
            self._stationary_vision_range_mismatch_identity = Some(identity);
            self._stationary_vision_range_mismatch_since_s = Some(time);
            return false;
        }
        self._stationary_vision_range_mismatch_since_s
            .is_some_and(|since| {
                time - since >= CORROBORATED_STATIONARY_VISION_RANGE_MISMATCH_HOLD_S
            })
    }
}

impl<P: crate::predictor::CutOutPredictor> Controller<P> {
    pub(super) fn reset_vision_transition(&mut self) {
        self._vision_transition_radar = None;
        self._vision_transition_previous = None;
        self._vision_transition_since_s = 0.;
    }

    /// Retain a measured front lead until its visual replacement is coherent.
    pub(super) fn confirm_vision_transition(
        &mut self,
        matched: Option<VisionMatch>,
        vision_lead: Option<&Lead>,
        points: &[Point],
        path: &Path,
        time: f64,
    ) -> Option<VisionMatch> {
        // Count camera time even while the ordinary matcher still accepts radar,
        // using the published visual velocity with the model ego-speed correction.
        match vision_lead {
            None => {
                self._vision_transition_previous = None;
                self._vision_transition_since_s = 0.;
            }
            Some(current) => {
                let continuous =
                    self._vision_transition_previous
                        .as_ref()
                        .is_some_and(|(previous_s, lead)| {
                            let dt = time - previous_s;
                            (0. ..=RADAR_MOTION_MAX_TIME_SKEW_S).contains(&dt)
                                && (current.d_rel - (lead.d_rel + lead.v_rel * dt)).abs()
                                    <= MOVING_FRONT_RANGE_MAX_POSITION_ERROR_M
                                && (current.y_rel - lead.y_rel).abs()
                                    <= MOVING_FRONT_RANGE_MAX_LATERAL_ERROR_M
                                && (current.v_lead - lead.v_lead).abs()
                                    <= MOVING_FRONT_RANGE_MAX_SPEED_JUMP_MPS
                        });
                if !continuous {
                    self._vision_transition_since_s = time;
                }
                self._vision_transition_previous = Some((time, current.clone()));
            }
        }
        let mut point = None;
        if let Some((previous_s, previous)) = self._vision_transition_radar.take() {
            let dt = time - previous_s;
            let speed_jump = if previous.v_lead.abs() <= STATIONARY_MAX_ABS_VLEAD_MPS {
                STATIONARY_FRONT_ANCHOR_MAX_SPEED_JUMP_MPS
            } else {
                MOVING_FRONT_RANGE_MAX_SPEED_JUMP_MPS
            };
            point = points
                .iter()
                .find(|candidate| {
                    candidate.source == previous.source && candidate.track_id == previous.track_id
                })
                .filter(|candidate| {
                    candidate.measured
                        && (0. ..=RADAR_MOTION_MAX_TIME_SKEW_S).contains(&dt)
                        && 0.5 < candidate.d_rel
                        && candidate.d_rel < 180.
                        && (candidate.d_rel - (previous.d_rel + previous.v_rel * dt)).abs()
                            <= MOVING_FRONT_RANGE_MAX_POSITION_ERROR_M
                        && (candidate.y_rel - (previous.y_rel + previous.yv_rel * dt)).abs()
                            <= MOVING_FRONT_RANGE_MAX_LATERAL_ERROR_M
                        && (candidate.v_lead - previous.v_lead).abs() <= speed_jump
                        && (candidate.v_lead > STATIONARY_MAX_ABS_VLEAD_MPS
                            || self.primary_matcher.stationary_identity.as_ref()
                                == Some(&candidate.identity()))
                        && path.project(candidate.d_rel, candidate.y_rel).d_path.abs()
                            <= RADAR_VISION_FALLBACK_MAX_ABS_DPATH_M
                });
        }
        let Some(point) = point
            .filter(|_| matched.is_none() && vision_lead.is_some() && self.enable_radar_tracks > 0)
            .cloned()
        else {
            if let Some(found) = matched.as_ref().filter(|found| {
                self.enable_radar_tracks > 0
                    && found.point.source == "frontRadar"
                    && found.point.measured
                    && found.d_path.abs() <= RADAR_VISION_FALLBACK_MAX_ABS_DPATH_M
            }) {
                self._vision_transition_radar = Some((time, found.point.clone()));
            }
            return matched;
        };
        // Both histories are current. A visual jump restarts confirmation while
        // the retained radar supplies its latest measured distance and kinematics.
        if time - self._vision_transition_since_s >= VISION_TRANSITION_CONFIRM_S - 1e-6 {
            self._vision_transition_previous = None;
            self._vision_transition_since_s = 0.;
            return None;
        }
        let d_path = path.project(point.d_rel, point.y_rel).d_path;
        self._vision_transition_radar = Some((time, point.clone()));
        Some(VisionMatch {
            point,
            probability: 0.,
            score: 0.,
            d_path,
        })
    }
}
