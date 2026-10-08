use crate::{lead_filter::LeadFilter, point::Source, scalar::FirstOrder};
use serde::Serialize;
use std::collections::VecDeque;

#[derive(Clone, Serialize)]
pub struct Track {
    pub track_id: u64,
    pub reused_corner_slot: bool,
    pub radar_source: Source,
    pub cnt: u64,
    #[serde(rename = "dRel")]
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub d_rel: f64,
    #[serde(rename = "vRel")]
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub v_rel: f64,
    #[serde(rename = "yRel")]
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub y_rel: f64,
    #[serde(rename = "yvRel")]
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub yv_rel: f64,
    #[serde(rename = "vLead")]
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub v_lead: f64,
    #[serde(rename = "aLead")]
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub a_lead: f64,
    #[serde(rename = "jLead")]
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub j_lead: f64,
    pub noisy: bool,
    #[serde(serialize_with = "crate::scalar::serialize_float")]
    pub dt: f64,
    #[serde(rename = "vLead_avg")]
    pub v_lead_avg: FirstOrder,
    #[serde(rename = "aLead_avg")]
    pub a_lead_avg: FirstOrder,
    #[serde(rename = "jLead_avg")]
    pub j_lead_avg: FirstOrder,
    #[serde(rename = "yRel_avg")]
    pub y_rel_avg: FirstOrder,
    #[serde(rename = "yvRel_avg")]
    pub yv_rel_avg: FirstOrder,
    pub lead_filter: LeadFilter,
    #[serde(
        rename = "aLead_v_history",
        serialize_with = "crate::scalar::serialize_history"
    )]
    pub a_lead_v_history: VecDeque<f64>,
    #[serde(
        rename = "jLead_v_history",
        serialize_with = "crate::scalar::serialize_history"
    )]
    pub j_lead_v_history: VecDeque<f64>,
    #[serde(skip)]
    pub(super) jerk_history_samples: usize,
    #[serde(skip)]
    pub(super) numpy_period: bool,
}
