use crate::lead::Lead;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Radar {
    pub lead_one: Lead,
    pub lead_two: Lead,
    pub lead_cut_in_risk: Lead,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(transparent)]
pub struct RadarSource(pub u16);

impl RadarSource {
    pub const fn corner(self) -> bool {
        matches!(self.0, 2..=4)
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct RadarPoint {
    pub track_id: u64,
    pub measured: bool,
    pub d_rel: f64,
    pub v_rel: f64,
    pub a_rel: f64,
    pub a_lead: f64,
    pub j_lead: f64,
    pub radar_source: RadarSource,
}
