use super::{
    constants::*, source::position_continuous, stationary_geometry::vision_base_cost, Matcher,
    VisionMatch,
};
use crate::{math::maximum, model::VisionLead};

impl Matcher {
    pub fn closer_handoff_ready(
        &mut self,
        stationary: Option<&VisionMatch>,
        moving: Option<&VisionMatch>,
        vision: Option<VisionLead>,
        time: Option<f64>,
    ) -> bool {
        let continuing = moving.zip(time).is_some_and(|(moving, now)| {
            self._stationary_closer_challenger_identity.as_ref() == Some(&moving.point.identity())
                && self._stationary_closer_challenger_since_s.is_some()
                && self
                    ._stationary_closer_challenger_last_point
                    .as_ref()
                    .zip(self._stationary_closer_challenger_last_time_s)
                    .is_some_and(|(previous, time)| {
                        position_continuous(previous, time, &moving.point, now)
                    })
        });
        let mut gain = STATIONARY_CLOSER_HANDOFF_MIN_COST_GAIN;
        if continuing
            && self
                ._stationary_closer_challenger_last_time_s
                .zip(self._stationary_closer_challenger_since_s)
                .is_some_and(|(last, since)| {
                    last - since >= STATIONARY_CLOSER_HANDOFF_COST_HOLD_MIN_S
                })
        {
            gain = STATIONARY_CLOSER_HANDOFF_HOLD_COST_GAIN;
        }
        let held_cost =
            stationary.and_then(|stationary| vision_base_cost(vision, &stationary.point));
        let challenger_cost = moving.and_then(|moving| vision_base_cost(vision, &moving.point));
        let cost_supported = stationary.zip(moving).is_some_and(|(stationary, moving)| {
            (stationary.point.y_rel - moving.point.y_rel).abs()
                <= STATIONARY_CLOSER_HANDOFF_MAX_YREL_DELTA_M
                && held_cost
                    .zip(challenger_cost)
                    .is_some_and(|(held, challenger)| challenger + gain <= held)
        });
        let range_supported =
            stationary
                .zip(moving)
                .zip(vision)
                .is_some_and(|((stationary, moving), vision)| {
                    (stationary.point.y_rel - moving.point.y_rel).abs()
                        <= STATIONARY_CLOSER_HANDOFF_RANGE_MAX_YREL_DELTA_M
                        && moving.d_path.abs() <= STATIONARY_CLOSER_HANDOFF_MAX_DPATH_M
                        && (moving.point.y_rel - vision.y_rel).abs()
                            <= STATIONARY_CLOSER_HANDOFF_MAX_VISION_YREL_ERROR_M
                        && (moving.point.d_rel - vision.d_rel).abs()
                            + STATIONARY_CLOSER_HANDOFF_MIN_VISION_RANGE_GAIN_M
                            <= (stationary.point.d_rel - vision.d_rel).abs()
                });
        let distinct =
            stationary
                .zip(moving)
                .zip(vision)
                .is_some_and(|((stationary, moving), vision)| {
                    cost_supported
                        && range_supported
                        && moving.point.radar_track_state
                            >= STATIONARY_RADAR_ONLY_FRONT_MIN_TRACK_STATE
                        && moving.score >= VISION_MATCH_FRESH_MIN_SCORE
                        && (moving.point.d_rel - vision.d_rel).abs()
                            <= STATIONARY_DISTINCT_HANDOFF_MAX_VISION_ERROR_M
                        && (moving.point.d_rel - vision.d_rel).abs()
                            <= STATIONARY_DISTINCT_HANDOFF_MAX_ERROR_RATIO
                                * (stationary.point.d_rel - vision.d_rel).abs()
                        && stationary.point.d_rel - moving.point.d_rel
                            <= VISION_RADAR_MAX_DISTANCE_ERROR_M
                });
        let eligible = stationary.zip(moving).zip(vision).zip(time).filter(
            |(((stationary, moving), vision), now)| {
                vision.probability >= STATIONARY_CLOSER_HANDOFF_MIN_VISION_PROB
                    && now.is_finite()
                    && stationary.point.identity() != moving.point.identity()
                    && stationary.point.source == "frontRadar"
                    && moving.point.source == stationary.point.source
                    && stationary.point.measured
                    && moving.point.measured
                    && stationary.point.v_lead.abs() <= STATIONARY_MAX_ABS_VLEAD_MPS
                    && moving.point.v_lead.abs() <= STATIONARY_MAX_ABS_VLEAD_MPS
                    && STATIONARY_CLOSER_HANDOFF_MIN_DREL_GAIN_M
                        <= stationary.point.d_rel - moving.point.d_rel
                    && (stationary.point.d_rel - moving.point.d_rel
                        <= STATIONARY_CLOSER_HANDOFF_MAX_DREL_DELTA_M
                        || distinct)
                    && (stationary.point.v_lead - moving.point.v_lead).abs()
                        <= STATIONARY_CLOSER_HANDOFF_MAX_VLEAD_DELTA_MPS
                    && (cost_supported || range_supported)
            },
        );
        let Some((((stationary, moving), _), now)) = eligible else {
            self.reset_stationary_challenger();
            return false;
        };
        if !continuing {
            self._stationary_closer_challenger_identity = Some(moving.point.identity());
            self._stationary_closer_challenger_since_s = Some(now);
        }
        self._stationary_closer_challenger_last_point = Some(moving.point.clone());
        self._stationary_closer_challenger_last_time_s = Some(now);
        let confirmation = if stationary.point.d_rel - moving.point.d_rel
            > STATIONARY_CLOSER_HANDOFF_MAX_DREL_DELTA_M
        {
            STATIONARY_DISTINCT_HANDOFF_CONFIRMATION_S
        } else {
            STATIONARY_CLOSER_HANDOFF_CONFIRMATION_S
        };
        self._stationary_closer_challenger_since_s
            .is_some_and(|since| now - since >= confirmation)
    }
    pub fn adopt_closer_handoff(&mut self, matched: &VisionMatch, vision: VisionLead, now: f64) {
        self.stationary_identity = Some(matched.point.identity());
        self._stationary_pending_identity = None;
        self._stationary_pending_since_s = None;
        self._stationary_pending_vision_support_frames = 0;
        self._stationary_pending_last_support_time_s = None;
        self._stationary_last_point = Some(matched.point.clone());
        self._stationary_last_time_s = Some(now);
        self._stationary_seed_probability = vision.probability;
        self._stationary_corner_supported = false;
        self._stationary_seed_score = vision_base_cost(Some(vision), &matched.point)
            .unwrap_or_else(|| maximum(0., 1. - matched.score));
        self._stationary_path_outlier_since_s = None;
        self._stationary_front_departure_since_s = None;
        self.reset_stationary_challenger();
    }
}
