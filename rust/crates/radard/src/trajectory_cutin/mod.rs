pub mod constants;
pub mod history;
mod policy;
mod support;
mod update;

use crate::{
    association::Matches,
    model::Model,
    path::Path,
    point::{Identity, Point, TrackId},
    Lead,
};
use history::Track;
use indexmap::IndexMap;
use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct Estimate {
    pub point: Point,
    pub continuity_id: u64,
    pub d_path: f64,
    pub d_path_rate: f64,
    pub future_d_rel: f64,
    pub future_d_path: f64,
    pub horizon_s: f64,
    pub time_to_overlap_s: Option<f64>,
    pub inward_rate: f64,
    pub reported_inward_rate: f64,
    pub inward_progress: f64,
    pub recent_inward_progress: f64,
    pub lateral_travel: f64,
    pub lateral_net_fraction: f64,
    pub direction_consistency: f64,
    pub recent_direction_consistency: f64,
    pub recent_v_rel_min: f64,
    pub recent_v_rel_spread: f64,
    pub recent_abs_yaw_max: f64,
    pub history_s: f64,
    pub confidence: f64,
    pub vision_supported: bool,
    pub cross_sensor_supported: bool,
    pub cross_sensor_track_id: Option<TrackId>,
    pub current_path: bool,
    pub raw_cutin: bool,
    pub confirmed_cutin: bool,
    pub control_eligible: bool,
    pub predecel_risk: bool,
    pub jittering: bool,
    pub unstable_fast_motion: bool,
    pub rear_pass: bool,
    pub parallel_drift: bool,
    pub front_history_supported: bool,
    pub close_front_supported: bool,
    pub curve_alias: bool,
    pub reason: &'static str,
    pub passing_before_overlap: bool,
    pub vision_bracket_supported: bool,
    pub paired_inward_motion_supported: bool,
    pub entry_withdrawn: bool,
    pub paired_body_entry: bool,
    pub stationary_pair_alias: bool,
}

impl Estimate {
    pub fn identity(&self) -> (String, TrackId, u64) {
        (
            self.point.source.clone(),
            self.point.track_id,
            self.continuity_id,
        )
    }
}

#[derive(Serialize)]
pub struct Detector {
    pub sensitivity: i32,
    #[serde(serialize_with = "crate::primary::entries")]
    pub _tracks: IndexMap<Identity, Track>,
    #[serde(serialize_with = "crate::primary::entries")]
    pub _corner_front_aliases: IndexMap<Identity, (Identity, f64)>,
    pub _next_continuity_id: u64,
    pub _last_time_s: Option<f64>,
    pub _last_v_ego: f64,
    pub _ego_distance: f64,
    pub last_estimates: Vec<Estimate>,
}

pub struct Frame<'a> {
    pub time_s: f64,
    pub v_ego: f64,
    pub points: &'a [Point],
    pub path: &'a Path,
    pub model: &'a Model,
    pub yaw_rate: f64,
    pub vision_required_front: bool,
    pub primary: Option<&'a Lead>,
    pub matches: Option<&'a Matches>,
}

impl Detector {
    pub fn new(sensitivity: i32) -> Self {
        Self {
            sensitivity: sensitivity.clamp(0, 5),
            _tracks: IndexMap::new(),
            _corner_front_aliases: IndexMap::new(),
            _next_continuity_id: 1,
            _last_time_s: None,
            _last_v_ego: 0.,
            _ego_distance: 0.,
            last_estimates: Vec::new(),
        }
    }
    pub fn reset(&mut self) {
        self._tracks.clear();
        self._corner_front_aliases.clear();
        self._last_time_s = None;
        self._last_v_ego = 0.;
        self._ego_distance = 0.;
        self.last_estimates.clear();
    }
}
