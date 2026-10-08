pub mod constants;
mod dynamics;
mod range;
mod scc;
mod update;

use crate::{
    association::Associator,
    model::Model,
    point::{Identity, Point},
    predictor::{CutOutPredictor, Predictor},
    primary::{self, Matcher},
    selection::{self, LeadTwoTracker, PrimaryHandoff, StationaryShadow},
    trajectory_cutin::Detector,
    trajectory_cutout, Lead,
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    pub prefer_corner_radar: bool,
    pub enable_radar_tracks: i32,
    pub cut_in_sensitivity: i32,
    pub front_radar_measurement_delay_s: f64,
    pub corner_radar_measurement_delay_s: f64,
    pub production_live_tracks: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            prefer_corner_radar: false,
            enable_radar_tracks: 1,
            cut_in_sensitivity: 3,
            front_radar_measurement_delay_s: 0.,
            corner_radar_measurement_delay_s: constants::CORNER_RADAR_MEASUREMENT_DELAY_S,
            production_live_tracks: false,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub time_s: f64,
    pub v_ego: f64,
    pub points: Vec<Point>,
    pub model: Model,
    #[serde(default)]
    pub yaw_rate_rad_s: f64,
    #[serde(default)]
    pub radar_to_model_time_s: f64,
}

#[derive(Clone, Default, Serialize)]
pub struct Output {
    pub lead_one: Option<Lead>,
    pub lead_two: Option<Lead>,
    pub lead_left: Option<Lead>,
    pub lead_right: Option<Lead>,
    pub leads_left: Vec<Lead>,
    pub leads_center: Vec<Lead>,
    pub leads_right: Vec<Lead>,
    pub leads_cutin: Vec<Lead>,
    pub leads_left2: Vec<Lead>,
    pub leads_right2: Vec<Lead>,
    pub lead_cutin_risk: Option<Lead>,
}

#[derive(Serialize)]
pub struct Controller<P: CutOutPredictor = Predictor> {
    pub primary_matcher: Matcher,
    pub scc_primary_fallback_matcher: Matcher,
    pub enable_radar_tracks: i32,
    pub front_radar_measurement_delay_s: f64,
    pub corner_radar_measurement_delay_s: f64,
    pub production_live_tracks: bool,
    pub motion_sensor: &'static str,
    pub cut_in_sensitivity: i32,
    pub trajectory_cutin: Detector,
    #[serde(serialize_with = "primary::entries")]
    pub _same_row_suppressed_until: IndexMap<selection::Identity, f64>,
    pub primary_cut_out_predictor: P,
    pub trajectory_cutout: trajectory_cutout::Tracker,
    #[serde(serialize_with = "serialize_associator")]
    pub front_kinematic_associator: Associator,
    pub lead_two_tracker: LeadTwoTracker,
    pub stationary_shadow_tracker: StationaryShadow,
    pub stationary_primary_handoff_tracker: PrimaryHandoff,
    pub scc_lead_two_tracker: scc::Tracker,
    pub lead_dynamics: dynamics::Dynamics,
    pub _stationary_vision_range_mismatch_identity: Option<Identity>,
    pub _stationary_vision_range_mismatch_since_s: Option<f64>,
    pub _moving_range_last_point: Option<Point>,
    pub _moving_range_last_time_s: Option<f64>,
    pub _stationary_range_last_point: Option<Point>,
    pub _stationary_range_last_time_s: Option<f64>,
    pub _stationary_range_anchor_time_s: Option<f64>,
}

fn serialize_associator<S: serde::Serializer>(
    value: &Associator,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    #[derive(Serialize)]
    struct Snapshot<'a> {
        #[serde(serialize_with = "primary::entries")]
        _pairs: &'a IndexMap<Identity, Identity>,
    }
    Snapshot {
        _pairs: &value.pairs,
    }
    .serialize(serializer)
}

impl<P: CutOutPredictor> Controller<P> {
    pub fn new(options: Options) -> Self {
        Self {
            primary_matcher: Matcher::default(),
            scc_primary_fallback_matcher: Matcher::default(),
            enable_radar_tracks: options.enable_radar_tracks,
            front_radar_measurement_delay_s: crate::math::maximum(
                0.,
                options.front_radar_measurement_delay_s,
            ),
            corner_radar_measurement_delay_s: crate::math::maximum(
                0.,
                options.corner_radar_measurement_delay_s,
            ),
            production_live_tracks: options.production_live_tracks,
            motion_sensor: if options.prefer_corner_radar {
                "corner"
            } else {
                "front"
            },
            cut_in_sensitivity: options.cut_in_sensitivity.clamp(0, 5),
            trajectory_cutin: Detector::new(options.cut_in_sensitivity.clamp(0, 5)),
            _same_row_suppressed_until: IndexMap::new(),
            primary_cut_out_predictor: P::default(),
            trajectory_cutout: trajectory_cutout::Tracker::default(),
            front_kinematic_associator: Associator::default(),
            lead_two_tracker: LeadTwoTracker::default(),
            stationary_shadow_tracker: StationaryShadow::default(),
            stationary_primary_handoff_tracker: PrimaryHandoff::default(),
            scc_lead_two_tracker: scc::Tracker::default(),
            lead_dynamics: dynamics::Dynamics::default(),
            _stationary_vision_range_mismatch_identity: None,
            _stationary_vision_range_mismatch_since_s: None,
            _moving_range_last_point: None,
            _moving_range_last_time_s: None,
            _stationary_range_last_point: None,
            _stationary_range_last_time_s: None,
            _stationary_range_anchor_time_s: None,
        }
    }
    fn reset_motion_pipeline(&mut self) {
        self.trajectory_cutin = Detector::new(self.cut_in_sensitivity);
        self._same_row_suppressed_until.clear();
    }
    fn reset_invalid_path(&mut self) {
        self.primary_matcher.reset();
        self.scc_primary_fallback_matcher.reset();
        self.lead_two_tracker.reset();
        self.stationary_shadow_tracker.reset();
        self.stationary_primary_handoff_tracker.reset();
        self.scc_lead_two_tracker.reset();
        self.primary_cut_out_predictor = P::default();
        self.trajectory_cutout.reset();
        self.lead_dynamics.reset();
        self.trajectory_cutin.reset();
        self.reset_range_mismatch();
        self._moving_range_last_point = None;
        self._moving_range_last_time_s = None;
        self.reset_stationary_range();
    }
    fn points_at_model_time(&self, points: &[Point], v_ego: f64, skew: f64) -> Vec<Point> {
        if !skew.is_finite() || skew.abs() > constants::RADAR_MOTION_MAX_TIME_SKEW_S {
            return Vec::new();
        }
        points
            .iter()
            .filter(|point| point.measured)
            .map(|point| {
                let source = point.source.rsplit('.').next().unwrap_or("");
                let delay = if source.starts_with("corner") {
                    self.corner_radar_measurement_delay_s
                } else {
                    self.front_radar_measurement_delay_s
                };
                point.aligned(v_ego, skew + delay)
            })
            .collect()
    }
    fn select_motion_points(&mut self, points: &[Point]) -> Vec<Point> {
        let corners: Vec<_> = points
            .iter()
            .filter(|point| point.corner())
            .cloned()
            .collect();
        if self.motion_sensor == "front" && !corners.is_empty() {
            self.motion_sensor = "corner";
            self.reset_motion_pipeline();
            self.lead_two_tracker.reset();
            self.stationary_shadow_tracker.reset();
            self.stationary_primary_handoff_tracker.reset();
            self.scc_lead_two_tracker.reset();
        }
        if self.motion_sensor == "corner" {
            corners
        } else {
            points
                .iter()
                .filter(|point| point.source == "frontRadar")
                .cloned()
                .collect()
        }
    }
    fn radar_lead(
        &self,
        point: &Point,
        d_path: f64,
        probability: f64,
        score: f64,
    ) -> Result<Lead, crate::Error> {
        let mut lead = crate::lead::from_point(point, d_path, probability, score)?;
        lead.a_lead_tau = self.lead_dynamics.tau(point);
        Ok(lead)
    }
}
