use super::{
    constants::*,
    history::{Observation, Track},
    policy::{evaluate, Evaluation},
    Detector, Estimate, Frame,
};
use crate::{
    math::{maximum, minimum},
    point::Identity,
    Error,
};
use std::collections::HashSet;

impl Detector {
    fn next_id(&mut self) -> Result<u64, Error> {
        let value = self._next_continuity_id;
        self._next_continuity_id = value
            .checked_add(1)
            .ok_or(Error::Contract("trajectory continuity identity exhausted"))?;
        Ok(value)
    }
    pub fn update(&mut self, frame: Frame<'_>) -> Result<&[Estimate], Error> {
        let now = frame.time_s;
        let speed = maximum(0., frame.v_ego);
        if let Some(last) = self._last_time_s {
            let dt = now - last;
            let backwards = dt < 0.;
            let stale = dt > 0.50;
            if backwards || stale {
                self.reset();
            } else {
                self._ego_distance += 0.5 * (self._last_v_ego + speed) * dt;
            }
        }
        self._last_time_s = Some(now);
        self._last_v_ego = speed;
        let vision = frame.model.primary_vision();
        let lane_supported = frame.model.lane_probabilities.len() < 3
            || maximum(
                frame.model.lane_probabilities[1],
                frame.model.lane_probabilities[2],
            ) >= 0.75;
        let mut seen = HashSet::new();
        let mut estimates = Vec::new();
        for point in frame.points {
            let unconfirmed_front = point.source == "frontRadar" && point.d_rel > 0.8;
            if !(point.measured || unconfirmed_front)
                || !(-5. <= point.d_rel && point.d_rel <= MAX_DREL_M)
            {
                continue;
            }
            let projection = frame.path.project(point.d_rel, point.y_rel);
            if projection.d_path.abs() > MOTION_SCOPE_HALF_WIDTH_M {
                continue;
            }
            let raw = point.identity();
            let cross = frame.matches.and_then(|matches| matches.get(&raw));
            let stable = cross
                .filter(|front| {
                    point.corner()
                        && front.source == "frontRadar"
                        && point.d_rel <= CROSS_SENSOR_SLOT_HANDOFF_MAX_DREL_M
                })
                .map(|front| Identity(format!("{}:front", point.source), front.track_id));
            let prior = self._corner_front_aliases.get(&raw).cloned();
            let stable = if let Some(stable) = stable {
                self._corner_front_aliases
                    .insert(raw.clone(), (stable.clone(), now));
                Some(stable)
            } else {
                prior
                    .as_ref()
                    .filter(|(_, time)| {
                        point.corner()
                            && point.d_rel <= CROSS_SENSOR_SLOT_HANDOFF_MAX_DREL_M
                            && now - time <= CROSS_SENSOR_ALIAS_HOLD_S
                    })
                    .map(|(key, _)| key.clone())
            };
            let mut key = stable.unwrap_or_else(|| raw.clone());
            if seen.contains(&key) {
                key = raw.clone();
            }
            seen.insert(key.clone());
            if !self._tracks.contains_key(&key) && key != raw {
                let history = prior.as_ref().map_or(&raw, |(key, _)| key);
                if self
                    ._tracks
                    .get(history)
                    .is_some_and(|state| state.continuous(point, now))
                {
                    let state = self
                        ._tracks
                        .shift_remove(history)
                        .ok_or(Error::Contract("trajectory transfer source vanished"))?;
                    self._tracks.insert(key.clone(), state);
                }
            }
            if !self._tracks.contains_key(&key) {
                let id = self.next_id()?;
                self._tracks.insert(key.clone(), Track::new(id));
            } else if self
                ._tracks
                .get(&key)
                .is_some_and(|state| !state.continuous(point, now))
            {
                let id = self.next_id()?;
                self._tracks
                    .get_mut(&key)
                    .ok_or(Error::Contract("trajectory reset state vanished"))?
                    .reset(id);
            }
            let state = self
                ._tracks
                .get_mut(&key)
                .ok_or(Error::Contract("trajectory active state vanished"))?;
            state.observations.push_back(Observation {
                time_s: now,
                d_rel: point.d_rel,
                y_rel: point.y_rel,
                v_rel: point.v_rel,
                v_lead: point.v_lead,
                yaw_rate_rad_s: frame.yaw_rate,
                global_path_s: self._ego_distance + projection.path_s,
                d_path: projection.d_path,
            });
            state.minimum_d_rel = minimum(state.minimum_d_rel, point.d_rel);
            if let Some(front) = cross {
                if state.front_track_id != Some(front.track_id)
                    || state
                        .front_observations
                        .back()
                        .is_some_and(|last| now - last.time_s > MAX_OBSERVATION_GAP_S)
                {
                    state.front_observations.clear();
                }
                state.front_track_id = Some(front.track_id);
                let projection = frame.path.project(front.d_rel, front.y_rel);
                state.front_observations.push_back(Observation {
                    time_s: now,
                    d_rel: front.d_rel,
                    y_rel: front.y_rel,
                    v_rel: front.v_rel,
                    v_lead: front.v_lead,
                    yaw_rate_rad_s: frame.yaw_rate,
                    global_path_s: self._ego_distance + projection.path_s,
                    d_path: projection.d_path,
                });
            }
            while state
                .front_observations
                .front()
                .is_some_and(|first| now - first.time_s > MAX_HISTORY_S)
            {
                state.front_observations.pop_front();
            }
            while state
                .observations
                .front()
                .is_some_and(|first| now - first.time_s > MAX_HISTORY_S)
            {
                state.observations.pop_front();
            }
            state.last_seen_s = now;
            estimates.push(evaluate(
                state,
                Evaluation {
                    frame: &frame,
                    point,
                    cross,
                    projection,
                    vision,
                    speed,
                    lane_supported,
                    sensitivity: self.sensitivity,
                },
            )?);
        }
        self._tracks.retain(|key, state| {
            let expired = now - state.last_seen_s > MAX_OBSERVATION_GAP_S;
            seen.contains(key) || !expired
        });
        self._corner_front_aliases
            .retain(|_, (_, time)| now - *time <= CROSS_SENSOR_ALIAS_HOLD_S);
        self.last_estimates = estimates;
        Ok(&self.last_estimates)
    }
}
