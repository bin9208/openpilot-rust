use serde::{Deserialize, Serialize};

pub const DT: f64 = 0.05;
pub const MIN_SPEED: f64 = 30.0 / 3.6;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum State {
    #[default]
    Off,
    PreLaneChange,
    Starting,
    Finishing,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum Maneuver {
    #[default]
    None,
    Turn,
    LaneChange,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Config {
    pub need_torque: i32,
    pub bsd: i32,
    pub line_check: i32,
    pub delay_tenths: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Car {
    pub can_valid: bool,
    pub left_blinker: bool,
    pub right_blinker: bool,
    pub v_ego: f64,
    pub a_ego: f64,
    pub trailer_connected: bool,
    pub steering_torque: f64,
    pub steering_pressed: bool,
    pub steering_angle_deg: f64,
    pub left_lane_line: i32,
    pub right_lane_line: i32,
    pub left_blindspot: bool,
    pub right_blindspot: bool,
}

impl Default for Car {
    fn default() -> Self {
        Self {
            can_valid: true,
            left_blinker: false,
            right_blinker: false,
            v_ego: 20.0,
            a_ego: 0.0,
            trailer_connected: false,
            steering_torque: 0.0,
            steering_pressed: false,
            steering_angle_deg: 0.0,
            left_lane_line: 0,
            right_lane_line: 0,
            left_blindspot: false,
            right_blindspot: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Model {
    pub lane_lines: [Vec<f64>; 4],
    pub lane_line_probs: [f64; 4],
    pub road_edges: [Vec<f64>; 2],
    pub desire_state: [f64; 8],
    pub orientation_rate_z: Vec<f64>,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            lane_lines: [-5.4, -1.8, 1.8, 5.4].map(|value| vec![value; 33]),
            lane_line_probs: [1.0; 4],
            road_edges: [-7.2, 7.2].map(|value| vec![value; 33]),
            desire_state: [0.0; 8],
            orientation_rate_z: vec![0.0; 33],
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Lead {
    pub status: bool,
    pub d_rel: f64,
    pub v_rel: Option<f64>,
    pub v_lead: Option<f64>,
    pub radar_track_id: i64,
}

impl Default for Lead {
    fn default() -> Self {
        Self {
            status: false,
            d_rel: 255.0,
            v_rel: None,
            v_lead: None,
            radar_track_id: -1,
        }
    }
}

impl Lead {
    pub fn v_relative(&self, v_ego: f64) -> f64 {
        self.v_rel.unwrap_or(self.v_lead.unwrap_or(v_ego) - v_ego)
    }

    pub fn corner(&self) -> bool {
        self.status
            && ((200..220).contains(&self.radar_track_id)
                || (240..250).contains(&self.radar_track_id)
                || (300..412).contains(&self.radar_track_id)
                || self.radar_track_id >= 1000)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Navigation {
    pub atc_type: String,
    pub command_index: i64,
    pub command: String,
    pub argument: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Input {
    pub car: Car,
    pub model: Model,
    pub navigation: Navigation,
    pub leads: [Lead; 2],
    pub objects: [Vec<Lead>; 2],
    pub lateral_active: bool,
    pub lane_change_prob: f64,
}

impl Default for Input {
    fn default() -> Self {
        Self {
            car: Car::default(),
            model: Model::default(),
            navigation: Navigation::default(),
            leads: std::array::from_fn(|_| Lead::default()),
            objects: std::array::from_fn(|_| Vec::new()),
            lateral_active: true,
            lane_change_prob: 0.1,
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("desire input requires 33-point lane/edge geometry and 16 orientation samples")]
pub struct InvalidModel;

impl Input {
    pub fn validate(&self) -> Result<(), InvalidModel> {
        if self
            .model
            .lane_lines
            .iter()
            .chain(&self.model.road_edges)
            .any(|values| values.len() != 33)
            || self.model.orientation_rate_z.len() < 16
        {
            return Err(InvalidModel);
        }
        Ok(())
    }
}

pub(crate) fn minimum(first: f64, second: f64) -> f64 {
    if second < first {
        second
    } else {
        first
    }
}

pub(crate) fn maximum(first: f64, second: f64) -> f64 {
    if second > first {
        second
    } else {
        first
    }
}
