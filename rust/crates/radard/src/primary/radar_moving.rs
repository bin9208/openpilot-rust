use super::{
    constants::*,
    source::{position_continuous, stationary_source_rank},
    stationary_support::Candidate,
    Matcher, VisionMatch,
};
use crate::{
    association::prefer_front_current,
    math::maximum,
    path::Path,
    point::{Point, TrackId},
};

fn limit(distance: f64) -> f64 {
    if distance > RADAR_ONLY_MOVING_FAR_DREL_M {
        RADAR_ONLY_MOVING_FAR_DPATH_M
    } else if distance > RADAR_ONLY_MOVING_MID_DREL_M {
        RADAR_ONLY_MOVING_MID_DPATH_M
    } else {
        RADAR_ONLY_MOVING_NEAR_DPATH_M
    }
}
fn nearest<'a>(candidates: impl Iterator<Item = &'a Candidate>) -> Option<Candidate> {
    candidates
        .min_by(|left, right| {
            (stationary_source_rank(&left.0), left.0.d_rel, left.1.abs())
                .partial_cmp(&(
                    stationary_source_rank(&right.0),
                    right.0.d_rel,
                    right.1.abs(),
                ))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned()
}
fn result(candidate: Candidate, points: &[Point]) -> VisionMatch {
    VisionMatch {
        point: prefer_front_current(&candidate.0, points),
        probability: 0.,
        score: maximum(0., 1. - candidate.1.abs() / candidate.2),
        d_path: candidate.1,
    }
}

impl Matcher {
    pub fn confirmed_closer_moving(
        &mut self,
        candidates: &[Candidate],
        held: &Candidate,
        now: f64,
    ) -> Option<Candidate> {
        let challenger = nearest(candidates.iter().filter(|candidate| {
            candidate.0.source == "frontRadar"
                && candidate.0.track_id != TrackId(0)
                && candidate.1.abs() <= RADAR_ONLY_MOVING_CLOSER_SWITCH_MAX_DPATH_M
        }));
        let Some(challenger) = challenger else {
            self.reset_moving_challenger();
            return None;
        };
        let identity = challenger.0.identity();
        if identity == held.0.identity()
            || held.0.track_id == TrackId(0)
            || challenger.0.d_rel > held.0.d_rel - RADAR_ONLY_MOVING_CLOSER_SWITCH_MIN_GAP_M
        {
            self.reset_moving_challenger();
            return None;
        }
        let continuous = self._radar_only_moving_challenger_identity.as_ref() == Some(&identity)
            && self
                ._radar_only_moving_challenger_last_point
                .as_ref()
                .zip(self._radar_only_moving_challenger_last_time_s)
                .is_some_and(|(point, time)| position_continuous(point, time, &challenger.0, now));
        if !continuous {
            self._radar_only_moving_challenger_identity = Some(identity);
            self._radar_only_moving_challenger_since_s = Some(now);
        }
        self._radar_only_moving_challenger_last_point = Some(challenger.0.clone());
        self._radar_only_moving_challenger_last_time_s = Some(now);
        let confirmation = if challenger.0.radar_track_state == 1 {
            RADAR_ONLY_MOVING_TENTATIVE_CONFIRMATION_S
        } else {
            RADAR_ONLY_MOVING_CONFIRMATION_S
        };
        if self
            ._radar_only_moving_challenger_since_s
            .is_none_or(|since| now - since < confirmation)
        {
            None
        } else {
            Some(challenger)
        }
    }
    pub fn match_radar_moving(
        &mut self,
        points: &[Point],
        path: &Path,
        time: Option<f64>,
    ) -> Option<VisionMatch> {
        let Some(now) = time.filter(|now| now.is_finite()) else {
            self.reset_radar_only_moving();
            return None;
        };
        let mut candidates = Vec::new();
        for point in points {
            if stationary_source_rank(point) > 2
                || !(0.5 < point.d_rel && point.d_rel <= RADAR_ONLY_MOVING_MAX_DREL_M)
                || point.v_lead <= RADAR_ONLY_MOVING_MIN_VLEAD_MPS
                || (point.d_rel > RADAR_ONLY_MOVING_RECEDING_MAX_DREL_M
                    && point.v_rel > RADAR_ONLY_MOVING_RECEDING_VREL_MPS)
            {
                continue;
            }
            let d_path = path.project(point.d_rel, point.y_rel).d_path;
            let d_path_limit = limit(point.d_rel);
            let identity = point.identity();
            let front_supported = point.corner()
                && prefer_front_current(point, points)
                    .kinematics_source
                    .as_deref()
                    == Some("frontRadar");
            let acquired = self.radar_only_moving_identity.as_ref() == Some(&identity)
                || self._radar_only_moving_pending_identity.as_ref() == Some(&identity);
            if d_path.abs() > d_path_limit
                || (d_path - point.y_rel).abs() >= RADAR_ONLY_MOVING_MAX_PATH_Y_OFFSET_M
                || self.moving_rejected(point, now)
                || (point.corner()
                    && point.d_rel < RADAR_ONLY_MOVING_CORNER_MIN_ACQUISITION_DREL_M
                    && !front_supported
                    && !acquired)
            {
                continue;
            }
            candidates.push((point.clone(), d_path, d_path_limit));
        }
        if candidates.is_empty() {
            self.reset_radar_only_moving();
            return None;
        }
        if let Some((previous, time)) = self
            ._radar_only_moving_last_point
            .as_ref()
            .zip(self._radar_only_moving_last_time_s)
            .filter(|_| self.radar_only_moving_identity.is_some())
        {
            let continuous = nearest(candidates.iter().filter(|candidate| {
                (self.radar_only_moving_identity.as_ref() == Some(&candidate.0.identity())
                    || candidate.0.source != previous.source)
                    && position_continuous(previous, time, &candidate.0, now)
            }));
            if let Some(mut selected) = continuous {
                if let Some(closer) = self.confirmed_closer_moving(&candidates, &selected, now) {
                    selected = closer;
                    self.reset_moving_challenger();
                }
                self.update_moving_history(&selected.0, now);
                if !self.moving_longitudinal_consistency(&selected.0, now) {
                    self.reject_moving(&selected.0, now);
                    self.reset_radar_only_moving();
                    return None;
                }
                self.radar_only_moving_identity = Some(selected.0.identity());
                return Some(result(selected, points));
            }
            self.reset_radar_only_moving();
        }
        let selected = nearest(candidates.iter())?;
        let identity = selected.0.identity();
        let continuous = self._radar_only_moving_pending_identity.as_ref() == Some(&identity)
            && self
                ._radar_only_moving_last_point
                .as_ref()
                .zip(self._radar_only_moving_last_time_s)
                .is_some_and(|(point, time)| position_continuous(point, time, &selected.0, now));
        if !continuous {
            self._radar_only_moving_pending_identity = None;
        }
        self.update_moving_history(&selected.0, now);
        let confirmation = if selected.0.radar_track_state == 1 {
            RADAR_ONLY_MOVING_TENTATIVE_CONFIRMATION_S
        } else if selected.0.corner() && selected.0.d_rel > RADAR_ONLY_MOVING_FAR_DREL_M {
            RADAR_ONLY_MOVING_FAR_CORNER_CONFIRMATION_S
        } else {
            RADAR_ONLY_MOVING_CONFIRMATION_S
        };
        let complete = self
            ._radar_only_moving_pending_since_s
            .is_some_and(|since| now - since >= confirmation);
        let consistent = self.moving_longitudinal_consistency(&selected.0, now);
        if !complete || !consistent {
            if complete && !consistent {
                self.reject_moving(&selected.0, now);
                self.reset_radar_only_moving();
            }
            return None;
        }
        self.radar_only_moving_identity = Some(identity);
        Some(result(selected, points))
    }
}
