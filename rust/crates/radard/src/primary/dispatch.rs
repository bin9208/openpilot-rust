use super::{constants::*, stationary_geometry::equivalent, Frame, Matcher, VisionMatch};
use crate::{point::Point, scope::turning_entry_allowed};
use std::collections::HashSet;

impl Matcher {
    pub fn update(
        &mut self,
        frame: &Frame<'_>,
        stationary_points: Option<&[Point]>,
    ) -> Option<VisionMatch> {
        self.update_vision_fallback(frame.vision);
        let mut points = frame.points.to_vec();
        let mut stationary_points = stationary_points.unwrap_or(frame.points).to_vec();
        if frame.yaw_rate.is_finite()
            && frame.yaw_rate.abs() >= STATIONARY_TURN_MIN_ABS_YAW_RATE_RAD_S
        {
            stationary_points.retain(|point| {
                !(point.corner()
                    && self.stationary_identity.as_ref() != Some(&point.identity())
                    && frame.vision.is_none_or(|vision| {
                        vision.probability < STATIONARY_TURN_CORNER_MIN_VISION_PROB
                    })
                    && (point.v_lead.abs() <= STATIONARY_MAX_ABS_VLEAD_MPS
                        || !turning_entry_allowed(
                            point,
                            frame.path.project(point.d_rel, point.y_rel).d_path,
                            frame.yaw_rate,
                            false,
                        )))
            });
        }
        if let Some(now) = frame.time.filter(|now| now.is_finite()) {
            let mut current = HashSet::new();
            for point in points
                .iter()
                .chain(&stationary_points)
                .filter(|point| point.measured)
            {
                let identity = point.identity();
                if !current.insert(identity.clone()) {
                    continue;
                }
                if self._observed_last_s.get(&identity).is_none_or(|last| {
                    now < *last || now - last > VISION_CORROBORATED_MAX_OBSERVATION_GAP_S
                }) {
                    self._observed_since_s.insert(identity.clone(), now);
                }
                self._observed_last_s.insert(identity, now);
            }
            let stale: Vec<_> = self
                ._observed_last_s
                .iter()
                .filter(|(_, last)| now - *last > VISION_CORROBORATED_MAX_OBSERVATION_GAP_S)
                .map(|(identity, _)| identity.clone())
                .collect();
            for identity in stale {
                self._observed_since_s.shift_remove(&identity);
                self._observed_last_s.shift_remove(&identity);
            }
        }
        self.update_front_evidence(
            frame.vision,
            &stationary_points,
            frame.path,
            frame.time,
            frame.yaw_rate,
        );
        let conflicts = self.moving_vision_conflicts(
            frame.vision,
            &stationary_points,
            frame.path,
            frame.time,
            frame.yaw_rate,
        );
        if !conflicts.is_empty() {
            for identity in &conflicts {
                self._stationary_front_evidence.shift_remove(identity);
            }
            points.retain(|point| !conflicts.contains(&point.identity()));
            stationary_points.retain(|point| !conflicts.contains(&point.identity()));
            if self
                .stationary_identity
                .as_ref()
                .is_some_and(|identity| conflicts.contains(identity))
                || self
                    ._stationary_pending_identity
                    .as_ref()
                    .is_some_and(|identity| conflicts.contains(identity))
            {
                self.reset_stationary();
            }
            if self
                .last_identity
                .as_ref()
                .is_some_and(|identity| conflicts.contains(identity))
            {
                self.reset_moving();
            }
        }
        let stationary_frame = Frame {
            points: &stationary_points,
            ..*frame
        };
        let mut stationary = self.match_stationary(&stationary_frame);
        let moving = self.match_moving(frame.vision, &points, frame.path);
        let outputs: Vec<_> = stationary_points
            .iter()
            .filter(|point| {
                frame
                    .allowed_output_sources
                    .is_none_or(|allowed| allowed.contains(&point.source))
            })
            .cloned()
            .collect();
        let far = self.match_far_corroborated(frame.vision, &outputs, frame.path, frame.time);
        if self.closer_handoff_ready(
            stationary.as_ref(),
            moving.as_ref(),
            frame.vision,
            frame.time,
        ) {
            if let Some(((matched, vision), now)) =
                moving.as_ref().zip(frame.vision).zip(frame.time)
            {
                self.adopt_closer_handoff(matched, vision, now);
                stationary = Some(matched.clone());
            }
        }
        let regular = if let Some((stationary, moving)) = stationary
            .as_ref()
            .zip(moving.as_ref())
            .filter(|(stationary, moving)| {
                stationary.point.identity() != moving.point.identity()
                    && !equivalent(&stationary.point, &moving.point)
                    && moving.point.v_lead.abs() > STATIONARY_MAX_ABS_VLEAD_MPS
            }) {
            let closer = stationary.point.source == "frontRadar"
                && stationary.point.measured
                && stationary.point.radar_track_state
                    >= STATIONARY_RADAR_ONLY_FRONT_MIN_TRACK_STATE
                && stationary.point.d_rel + STATIONARY_CLOSER_THAN_MOVING_MIN_DREL_GAIN_M
                    < moving.point.d_rel
                && stationary.d_path.abs() <= STATIONARY_RADAR_ONLY_FRONT_FAR_MAX_DPATH_M;
            if closer {
                Some(stationary.clone())
            } else {
                self.reset_stationary();
                Some(moving.clone())
            }
        } else {
            stationary.or_else(|| moving.clone())
        };
        let radar_moving = self.match_radar_moving(&outputs, frame.path, frame.time);
        if let Some(regular) = regular {
            if let Some((_, radar)) = moving
                .as_ref()
                .zip(radar_moving.as_ref())
                .filter(|(moving, radar)| radar.point.d_rel < moving.point.d_rel)
            {
                self.last_identity = Some(radar.point.identity());
                self.low_probability_hold_frames = 0;
                return Some(radar.clone());
            }
            return Some(regular);
        }
        let corroborated = self
            .match_corroborated(frame.vision, &outputs, frame.path, frame.time)
            .or(far);
        if radar_moving.as_ref().is_some_and(|radar| {
            corroborated
                .as_ref()
                .is_none_or(|corroborated| radar.point.d_rel < corroborated.point.d_rel)
        }) {
            radar_moving
        } else {
            corroborated
        }
    }
}
