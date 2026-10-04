use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Lead {
    pub d_rel: f64,
    pub y_rel: f64,
    pub v_rel: f64,
    pub a_rel: f64,
    pub v_lead: f64,
    pub d_path: f64,
    pub v_lat: f64,
    pub v_lead_k: f64,
    pub a_lead_k: f64,
    pub fcw: bool,
    pub status: bool,
    pub a_lead_tau: f64,
    pub model_prob: f64,
    pub radar: bool,
    pub radar_track_id: i32,
    pub a_lead: f64,
    pub j_lead: f64,
    pub score: f64,
    pub cut_out_time: f64,
    pub cut_out_confidence: f64,
}

impl Default for Lead {
    fn default() -> Self {
        Self {
            d_rel: 0.,
            y_rel: 0.,
            v_rel: 0.,
            a_rel: 0.,
            v_lead: 0.,
            d_path: 0.,
            v_lat: 0.,
            v_lead_k: 0.,
            a_lead_k: 0.,
            fcw: false,
            status: false,
            a_lead_tau: 0.,
            model_prob: 0.,
            radar: false,
            radar_track_id: -1,
            a_lead: 0.,
            j_lead: 0.,
            score: 0.,
            cut_out_time: 0.,
            cut_out_confidence: 0.,
        }
    }
}

impl Lead {
    pub const fn signature(&self) -> (bool, bool, i32) {
        (self.status, self.radar, self.radar_track_id)
    }
}
