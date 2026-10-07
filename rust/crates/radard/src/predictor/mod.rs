pub mod history;
mod prediction;
mod state_snapshot;

use crate::{
    math::finite,
    path::Path,
    point::{velocity_in_ego_frame, Identity, Point},
    scope, Error,
};
use history::{Observation, Track};
use indexmap::IndexMap;
use std::collections::HashSet;

pub struct Predictor {
    pub states: IndexMap<Identity, Track>,
    pub retired: IndexMap<u64, Track>,
    pub next_continuity_id: u64,
    pub ego_distance: f64,
    pub ego_x: f64,
    pub ego_y: f64,
    pub ego_heading: f64,
    pub last_update: Option<f64>,
    pub last_v_ego: f64,
    pub last_yaw_rate: f64,
}

impl Default for Predictor {
    fn default() -> Self {
        Self {
            states: IndexMap::new(),
            retired: IndexMap::new(),
            next_continuity_id: 1,
            ego_distance: 0.,
            ego_x: 0.,
            ego_y: 0.,
            ego_heading: 0.,
            last_update: None,
            last_v_ego: 0.,
            last_yaw_rate: 0.,
        }
    }
}

pub struct Frame<'a> {
    pub time_s: f64,
    pub v_ego: f64,
    pub yaw_rate: f64,
    pub points: &'a [Point],
    pub path: &'a Path,
    pub requested: &'a HashSet<Identity>,
    pub scoped: Option<&'a [scope::Scoped<'a>]>,
}

impl Predictor {
    fn retire(&mut self, key: &Identity) {
        if let Some(state) = self.states.shift_remove(key) {
            self.retired.insert(state.continuity_id, state);
        }
    }

    pub fn update(&mut self, input: Frame<'_>) -> Result<IndexMap<Identity, f64>, Error> {
        let Frame {
            time_s: now,
            v_ego,
            yaw_rate,
            points,
            path,
            requested,
            scoped,
        } = input;
        let v_ego = finite(v_ego, 0.);
        let yaw_rate = finite(yaw_rate, 0.);
        if points.iter().any(|point| point.source != "frontRadar") {
            return Err(Error::Contract(
                "production cut-out predictor requires front radar points",
            ));
        }
        if let Some(previous) = self.last_update {
            let dt = now - previous;
            if dt > 0. {
                let distance = 0.5 * (self.last_v_ego + v_ego) * dt;
                let yaw = 0.5 * (self.last_yaw_rate + yaw_rate);
                let mid_heading = self.ego_heading + 0.5 * yaw * dt;
                self.ego_distance += distance;
                self.ego_x += distance * mid_heading.cos();
                self.ego_y += distance * mid_heading.sin();
                self.ego_heading += yaw * dt;
            }
        }
        self.last_update = Some(now);
        self.last_v_ego = v_ego;
        self.last_yaw_rate = yaw_rate;
        self.states.retain(|_, state| {
            let expired = now - state.last_seen_s > 0.35;
            !expired
        });
        self.retired.retain(|_, state| {
            let expired = now - state.last_seen_s > 0.35;
            !expired
        });
        let computed;
        let scoped = match scoped {
            Some(scoped) => scoped,
            None => {
                computed = scope::points(points, path);
                &computed
            }
        };
        let visible: HashSet<_> = scope::visible(scoped, None, &HashSet::new())
            .iter()
            .map(|point| point.identity())
            .collect();
        let scoped_keys: HashSet<_> = scoped.iter().map(|value| value.point.identity()).collect();
        for point in points {
            let identity = point.identity();
            if point.measured && !scoped_keys.contains(&identity) {
                self.retire(&identity);
            }
        }
        let ego_projection = path.project(0., 0.);
        let cos_heading = self.ego_heading.cos();
        let sin_heading = self.ego_heading.sin();
        let mut prepared = Vec::with_capacity(scoped.len());
        for value in scoped {
            let point = value.point;
            let identity = point.identity();
            let speed = finite(point.v_lead, finite(point.v_rel, 0.) + v_ego);
            if speed.abs() <= 2.5 {
                self.retire(&identity);
                continue;
            }
            let projection = value.projection;
            let lateral = finite(point.y_rel, 0.);
            let lateral_speed = finite(point.yv_rel, 0.);
            let [target_x, target_y] = velocity_in_ego_frame(
                &Point {
                    v_lead: speed,
                    d_rel: value.distance,
                    y_rel: lateral,
                    yv_rel: lateral_speed,
                    ..Point::default()
                },
                yaw_rate,
            );
            let observation = Observation {
                time_s: now,
                d_rel: value.distance,
                y_rel: lateral,
                v_rel: finite(point.v_rel, 0.),
                a_rel: finite(point.a_rel, 0.),
                v_lead: speed,
                a_lead: finite(point.a_lead, finite(point.a_rel, 0.)),
                yv_rel: lateral_speed,
                v_ego,
                ego_distance: self.ego_distance,
                ego_x: self.ego_x,
                ego_y: self.ego_y,
                ego_heading: self.ego_heading,
                ego_path_s: ego_projection.path_s,
                path_s: projection.path_s,
                path_x_world: self.ego_distance + projection.path_s - ego_projection.path_s,
                path_velocity: projection.tangent_x * target_x + projection.tangent_y * target_y,
                normal_velocity: -projection.tangent_y * target_x + projection.tangent_x * target_y,
                actual_world_x: self.ego_x + cos_heading * value.distance - sin_heading * lateral,
                actual_world_y: self.ego_y + sin_heading * value.distance + cos_heading * lateral,
                d_path: projection.d_path,
            };
            prepared.push((identity, observation));
        }
        let mut predictions = IndexMap::new();
        for (identity, observation) in prepared {
            if let Some(track) = self.states.get(&identity) {
                if !prediction::continuous(track, &observation)? {
                    self.retire(&identity);
                }
            }
            if !self.states.contains_key(&identity) {
                let track = Track::new(identity.0.clone(), self.next_continuity_id, now);
                self.next_continuity_id = self
                    .next_continuity_id
                    .checked_add(1)
                    .ok_or(Error::Contract("radar continuity ID overflow"))?;
                self.states.insert(identity.clone(), track);
            }
            let track = self
                .states
                .get_mut(&identity)
                .ok_or(Error::Contract("new radar history missing"))?;
            track.observations.push_back(observation);
            while track
                .observations
                .front()
                .is_some_and(|value| now - value.time_s > 2.)
            {
                track.observations.pop_front();
            }
            track.last_seen_s = now;
            if visible.contains(&identity) && requested.contains(&identity) {
                predictions.insert(identity, prediction::probability(track, &observation)?);
            } else if visible.contains(&identity) {
                let long = track.window(1.50);
                let span = if long.len() >= 2 {
                    long.iter()
                        .map(|value| value.path_x_world)
                        .fold(f64::NEG_INFINITY, crate::math::maximum)
                        - long
                            .iter()
                            .map(|value| value.path_x_world)
                            .fold(f64::INFINITY, crate::math::minimum)
                } else {
                    0.
                };
                track.occupancy(&observation, long.len() >= 4 && span >= 1.5);
            } else if track.inside_latched && observation.d_path.abs() > 1.8 + 0.12 {
                track.inside_latched = false;
                track.entry_time_s = None;
            }
        }
        Ok(predictions)
    }
}

pub trait CutOutPredictor: Default {
    fn predict(&mut self, frame: Frame<'_>) -> Result<IndexMap<Identity, f64>, Error>;
}

impl CutOutPredictor for Predictor {
    fn predict(&mut self, frame: Frame<'_>) -> Result<IndexMap<Identity, f64>, Error> {
        self.update(frame)
    }
}
