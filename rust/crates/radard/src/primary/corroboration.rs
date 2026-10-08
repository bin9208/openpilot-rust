use super::{
    constants::*,
    source::{position_continuous, stationary_source_rank},
    stationary_support::Candidate,
    Matcher, VisionMatch,
};
use crate::{
    math::{maximum, minimum},
    model::VisionLead,
    path::Path,
    point::Point,
};

fn choose(candidates: &[Candidate], vision_path: f64) -> Option<Candidate> {
    let rank = candidates
        .iter()
        .map(|(point, _, _)| stationary_source_rank(point))
        .min()?;
    candidates
        .iter()
        .filter(|(point, _, _)| stationary_source_rank(point) == rank)
        .min_by(|left, right| {
            (left.2, (left.1 - vision_path).abs(), left.0.track_id)
                .partial_cmp(&(right.2, (right.1 - vision_path).abs(), right.0.track_id))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned()
}

impl Matcher {
    pub fn match_corroborated(
        &mut self,
        vision: Option<VisionLead>,
        points: &[Point],
        path: &Path,
        time: Option<f64>,
    ) -> Option<VisionMatch> {
        let vision = vision?;
        if vision.probability < VISION_LEAD_MIN_PROB {
            return None;
        }
        let vision_path = path.project(vision.d_rel, vision.y_rel).d_path;
        let mut candidates = Vec::new();
        for point in points {
            if !point.measured
                || stationary_source_rank(point) > 2
                || !(0.5 < point.d_rel && point.d_rel < 180.)
            {
                continue;
            }
            let d_path = path.project(point.d_rel, point.y_rel).d_path;
            let stationary = point.v_lead.abs() <= STATIONARY_MAX_ABS_VLEAD_MPS;
            let identity = point.identity();
            let retained = point.corner()
                && (self.stationary_identity.as_ref() == Some(&identity)
                    || self._stationary_pending_identity.as_ref() == Some(&identity))
                && point.v_lead.abs() <= STATIONARY_HELD_CORNER_MAX_ABS_VLEAD_MPS;
            let speed_limit = if stationary {
                if point.corner() || point.source == "scc" {
                    STATIONARY_TRUSTED_MAX_VISION_SPEED_DELTA_MPS
                } else {
                    STATIONARY_MAX_VISION_SPEED_DELTA_MPS
                }
            } else {
                VISION_ONLY_CORROBORATION_MAX_VLEAD_DELTA_MPS
            };
            if (point.d_rel - vision.d_rel).abs() > VISION_RADAR_MAX_DISTANCE_ERROR_M
                || d_path.abs() > VISION_MATCH_FRESH_MAX_DPATH_M
                || (d_path - vision_path).abs() > VISION_ONLY_CORROBORATION_MAX_DPATH_DELTA_M
                || ((point.v_lead - vision.velocity).abs() > speed_limit && !retained)
                || (stationary && !point.corner())
            {
                continue;
            }
            candidates.push((point.clone(), d_path, (point.d_rel - vision.d_rel).abs()));
        }
        let (point, d_path, error) = choose(&candidates, vision_path)?;
        if time.is_some_and(|now| {
            now.is_finite()
                && point.corner()
                && (point.v_lead - vision.velocity).abs()
                    > maximum(5., vision.velocity.abs() * 0.30)
                && now
                    - self
                        ._observed_since_s
                        .get(&point.identity())
                        .copied()
                        .unwrap_or(now)
                    < VISION_CORROBORATED_MIN_OBSERVED_S
        }) {
            return None;
        }
        if point.v_lead.abs() > STATIONARY_MAX_ABS_VLEAD_MPS {
            self.last_identity = Some(point.identity());
            self.low_probability_hold_frames = 0;
        }
        Some(VisionMatch {
            point,
            probability: vision.probability,
            score: maximum(0., 1. - error / VISION_RADAR_MAX_DISTANCE_ERROR_M),
            d_path,
        })
    }
    pub fn match_far_corroborated(
        &mut self,
        vision: Option<VisionLead>,
        points: &[Point],
        path: &Path,
        time: Option<f64>,
    ) -> Option<VisionMatch> {
        let Some((vision, now)) =
            vision
                .zip(time.filter(|time| time.is_finite()))
                .filter(|(vision, _)| {
                    let below_hold = vision.probability < VISION_RADAR_FAR_MIN_HOLD_PROB;
                    !below_hold
                })
        else {
            self.reset_far_vision();
            return None;
        };
        let limit = minimum(
            VISION_RADAR_FAR_MAX_DISTANCE_ERROR_M,
            maximum(
                VISION_RADAR_MAX_DISTANCE_ERROR_M,
                vision.d_rel * VISION_RADAR_FAR_DISTANCE_ERROR_FRACTION,
            ),
        );
        if limit <= VISION_RADAR_MAX_DISTANCE_ERROR_M {
            self.reset_far_vision();
            return None;
        }
        let vision_path = path.project(vision.d_rel, vision.y_rel).d_path;
        let mut candidates = Vec::new();
        for point in points {
            if !point.measured
                || stationary_source_rank(point) > 2
                || !(0.5 < point.d_rel && point.d_rel < 180.)
                || point.v_lead <= STATIONARY_MAX_ABS_VLEAD_MPS
            {
                continue;
            }
            let error = (point.d_rel - vision.d_rel).abs();
            if error > limit
                || (point.v_lead - vision.velocity).abs() > VISION_RADAR_FAR_MAX_VLEAD_DELTA_MPS
            {
                continue;
            }
            let d_path = path.project(point.d_rel, point.y_rel).d_path;
            if d_path.abs() > VISION_RADAR_FAR_MAX_DPATH_M
                || (d_path - vision_path).abs() > VISION_ONLY_CORROBORATION_MAX_DPATH_DELTA_M
            {
                continue;
            }
            candidates.push((point.clone(), d_path, error));
        }
        let Some((point, d_path, error)) = choose(&candidates, vision_path) else {
            self.reset_far_vision();
            return None;
        };
        let identity = point.identity();
        let continuous = self._far_vision_radar_identity.as_ref() == Some(&identity)
            && self
                ._far_vision_radar_last_point
                .as_ref()
                .zip(self._far_vision_radar_last_time_s)
                .is_some_and(|(previous, time)| position_continuous(previous, time, &point, now));
        if !continuous {
            if vision.probability < VISION_RADAR_FAR_MIN_SEED_PROB {
                self.reset_far_vision();
                return None;
            }
            self._far_vision_radar_identity = Some(identity);
            self._far_vision_radar_since_s = Some(now);
        }
        self._far_vision_radar_last_point = Some(point.clone());
        self._far_vision_radar_last_time_s = Some(now);
        if self
            ._far_vision_radar_since_s
            .is_none_or(|since| now - since < VISION_RADAR_FAR_CONFIRMATION_S)
        {
            return None;
        }
        Some(VisionMatch {
            point,
            probability: vision.probability,
            score: maximum(0., 1. - error / limit),
            d_path,
        })
    }
}
