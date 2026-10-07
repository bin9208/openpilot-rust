use openpilot_cereal::log_capnp::{Desire, LaneChangeDirection, LaneChangeState};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Trajectory {
    pub t: Vec<f64>,
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    pub z: Vec<f64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelLead {
    pub prob: f64,
    pub x: Vec<f64>,
    pub v: Vec<f64>,
    pub x_std: Vec<f64>,
    pub y_std: Vec<f64>,
    pub v_std: Vec<f64>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelAction {
    pub desired_acceleration: f64,
    pub desired_velocity: f64,
    pub should_stop: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelMeta {
    #[serde(with = "enum_wire")]
    pub desire: Desire,
    #[serde(with = "enum_wire")]
    pub lane_change_state: LaneChangeState,
    #[serde(with = "enum_wire")]
    pub lane_change_direction: LaneChangeDirection,
    pub desire_state: Vec<f64>,
    pub desire_prediction: Vec<f64>,
    pub lane_width_left: f64,
    pub lane_width_right: f64,
    pub gas_press_probs: Vec<f64>,
}

impl Default for ModelMeta {
    fn default() -> Self {
        Self {
            desire: Desire::None,
            lane_change_state: LaneChangeState::Off,
            lane_change_direction: LaneChangeDirection::None,
            desire_state: Vec::new(),
            desire_prediction: Vec::new(),
            lane_width_left: 0.,
            lane_width_right: 0.,
            gas_press_probs: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Model {
    pub frame_id: u32,
    pub position: Trajectory,
    pub velocity: Trajectory,
    pub acceleration: Trajectory,
    pub orientation: Trajectory,
    pub orientation_rate: Trajectory,
    pub lane_lines: Vec<Trajectory>,
    pub road_edges: Vec<Trajectory>,
    pub lane_line_probs: Vec<f64>,
    pub lane_line_stds: Vec<f64>,
    pub road_edge_stds: Vec<f64>,
    pub leads_v3: Vec<ModelLead>,
    pub meta: ModelMeta,
    pub action: ModelAction,
}

pub mod enum_wire {
    use serde::{de::Error, Deserialize, Deserializer, Serializer};

    pub fn serialize<S, T>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: Copy + Into<u16>,
    {
        serializer.serialize_u16((*value).into())
    }
    pub fn deserialize<'de, D, T>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
        T: TryFrom<u16>,
        T::Error: std::fmt::Display,
    {
        T::try_from(u16::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}
