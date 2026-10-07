use crate::{
    math::{count, float_sum, maximum, minimum, square},
    Error,
};
use serde::Serialize;
use std::collections::VecDeque;

#[derive(Clone, Copy, Serialize)]
pub struct Observation {
    pub time_s: f64,
    pub d_rel: f64,
    pub y_rel: f64,
    pub v_rel: f64,
    pub a_rel: f64,
    pub v_lead: f64,
    pub a_lead: f64,
    pub yv_rel: f64,
    pub v_ego: f64,
    pub ego_distance: f64,
    pub ego_x: f64,
    pub ego_y: f64,
    pub ego_heading: f64,
    pub ego_path_s: f64,
    pub path_s: f64,
    pub path_x_world: f64,
    pub path_velocity: f64,
    pub normal_velocity: f64,
    pub actual_world_x: f64,
    pub actual_world_y: f64,
    pub d_path: f64,
}

#[derive(Clone, Serialize)]
pub struct Track {
    pub source: String,
    pub continuity_id: u64,
    pub observations: VecDeque<Observation>,
    pub inside_latched: bool,
    pub entry_time_s: Option<f64>,
    pub position_history_override_latched: bool,
    pub last_seen_s: f64,
}

impl Track {
    pub fn new(source: String, continuity_id: u64, now: f64) -> Self {
        Self {
            source,
            continuity_id,
            observations: VecDeque::new(),
            inside_latched: false,
            entry_time_s: None,
            position_history_override_latched: false,
            last_seen_s: now,
        }
    }
    pub fn window(&self, duration: f64) -> Vec<Observation> {
        let Some(latest) = self.observations.back() else {
            return Vec::new();
        };
        self.observations
            .iter()
            .filter(|observation| observation.time_s >= latest.time_s - duration)
            .copied()
            .collect()
    }
    pub fn occupancy(&mut self, observation: &Observation, enough_history: bool) {
        let was_inside = self.inside_latched;
        let limit = if self.inside_latched { 1.8 + 0.12 } else { 1.8 };
        self.inside_latched = observation.d_path.abs() <= limit;
        if !was_inside && self.inside_latched {
            self.entry_time_s = enough_history.then_some(observation.time_s);
        } else if !self.inside_latched {
            self.entry_time_s = None;
        }
    }
}

pub fn linear_fit(
    observations: &[Observation],
    field: fn(&Observation) -> f64,
) -> Result<[f64; 2], Error> {
    if observations.len() < 2 {
        return Ok([0.; 2]);
    }
    let count = count(observations.len())?;
    let time_mean = observations
        .iter()
        .fold(0., |sum, value| sum + value.time_s)
        / count;
    let value_mean = observations
        .iter()
        .fold(0., |sum, value| sum + field(value))
        / count;
    let mut denominator = 0.;
    for value in observations {
        denominator += square(value.time_s - time_mean)?;
    }
    if denominator < 1e-6 {
        return Ok([0.; 2]);
    }
    let numerator = observations.iter().fold(0., |sum, value| {
        sum + (value.time_s - time_mean) * (field(value) - value_mean)
    });
    let slope = numerator / denominator;
    let intercept = value_mean - slope * time_mean;
    let mut residual = 0.;
    for value in observations {
        residual += square(field(value) - (intercept + slope * value.time_s))?;
    }
    Ok([slope, (residual / count).sqrt()])
}

pub fn spatial_fit(observations: &[Observation]) -> Result<[f64; 3], Error> {
    if observations.len() < 2 {
        return Ok([0.; 3]);
    }
    let count = count(observations.len())?;
    let mut total_x = 0.;
    let mut total_y = 0.;
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for value in observations {
        total_x += value.path_x_world;
        total_y += value.d_path;
        low = minimum(low, value.path_x_world);
        high = maximum(high, value.path_x_world);
    }
    let x_mean = total_x / count;
    let y_mean = total_y / count;
    let mut denominator = 0.;
    for value in observations {
        denominator += square(value.path_x_world - x_mean)?;
    }
    let span = high - low;
    if denominator < 1e-6 {
        return Ok([0., 0., span]);
    }
    let numerator = observations.iter().fold(0., |sum, value| {
        sum + (value.path_x_world - x_mean) * (value.d_path - y_mean)
    });
    let slope = numerator / denominator;
    let intercept = y_mean - slope * x_mean;
    let mut residual = 0.;
    for value in observations {
        residual += square(value.d_path - (intercept + slope * value.path_x_world))?;
    }
    Ok([slope, (residual / count).sqrt(), span])
}

pub fn directional_metrics(track: &Track, d_path: f64) -> Result<[f64; 3], Error> {
    let values = track.window(0.80);
    if values.len() < 6 || d_path.abs() <= 1e-6 {
        return Ok([0.; 3]);
    }
    let side = 1_f64.copysign(d_path);
    let steps: Vec<_> = values
        .windows(2)
        .map(|pair| -side * (pair[1].d_path - pair[0].d_path))
        .collect();
    let travel = float_sum(steps.iter().map(|value| value.abs()));
    let inward = float_sum(steps.iter().copied());
    let consistency = if travel > 1e-6 {
        maximum(0., minimum(1., inward / travel))
    } else {
        0.
    };
    let ratio = count(steps.iter().filter(|value| **value > 0.).count())? / count(steps.len())?;
    Ok([maximum(0., inward), consistency, ratio])
}
