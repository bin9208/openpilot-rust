use super::{history::Track, Predictor};
use crate::point::Identity;
use indexmap::IndexMap;
use serde::{ser::SerializeStruct, Serialize, Serializer};
use std::collections::BTreeMap;

#[derive(Serialize)]
struct Active<'a> {
    #[serde(serialize_with = "crate::primary::entries")]
    front: &'a IndexMap<Identity, Track>,
    corner: BTreeMap<String, Track>,
}

#[derive(Serialize)]
struct Retired<'a> {
    #[serde(serialize_with = "crate::primary::entries")]
    front: &'a IndexMap<u64, Track>,
    corner: BTreeMap<String, Track>,
}

impl Serialize for Predictor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("RadarMotionPredictor", 12)?;
        state.serialize_field("cut_out_only", &true)?;
        state.serialize_field("directional_min_consistency", &0.75)?;
        state.serialize_field(
            "_states",
            &Active {
                front: &self.states,
                corner: BTreeMap::new(),
            },
        )?;
        state.serialize_field(
            "_retired_states",
            &Retired {
                front: &self.retired,
                corner: BTreeMap::new(),
            },
        )?;
        state.serialize_field("_next_continuity_id", &self.next_continuity_id)?;
        state.serialize_field("_ego_distance_m", &self.ego_distance)?;
        state.serialize_field("_ego_x_m", &self.ego_x)?;
        state.serialize_field("_ego_y_m", &self.ego_y)?;
        state.serialize_field("_ego_heading_rad", &self.ego_heading)?;
        state.serialize_field("_last_update_s", &self.last_update)?;
        state.serialize_field("_last_v_ego", &self.last_v_ego)?;
        state.serialize_field("_last_yaw_rate", &self.last_yaw_rate)?;
        state.end()
    }
}
