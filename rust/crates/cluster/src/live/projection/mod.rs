mod car;
mod change;
mod lanes;
mod model;
mod plan;
mod points;
mod radar;
mod risk;
mod trip;
mod value;
mod vehicle;

use super::Error;
use crate::models::{ClusterUiState, DetectedVehicle};
use capnp::dynamic_value::Reader;
pub(crate) use change::animate;
pub use lanes::Snapshot as LaneSnapshot;
use num_traits::ToPrimitive;
use openpilot_cereal::log_capnp::event;
pub(crate) use value::Data;

pub struct Projection {
    pub state: ClusterUiState,
    pub(crate) selfdrive_enabled: Option<bool>,
    controls_enabled: Option<bool>,
    v_ego_cluster_seen: bool,
    nav_limit: Option<i64>,
    nav_time: f64,
    controls_curvature: Option<f64>,
    model_curvature: Option<f64>,
    wheelbase: f64,
    steer_ratio: f64,
    trip: trip::Tracker,
    lanes: lanes::Lanes,
    change: change::Change,
    model_detections: Vec<DetectedVehicle>,
    model_detection_time: f64,
    radar: radar::Radar,
}

impl Default for Projection {
    fn default() -> Self {
        Self {
            state: ClusterUiState::default(),
            selfdrive_enabled: None,
            controls_enabled: None,
            v_ego_cluster_seen: false,
            nav_limit: None,
            nav_time: -999.0,
            controls_curvature: None,
            model_curvature: None,
            wheelbase: 2.7,
            steer_ratio: 15.0,
            trip: trip::Tracker::default(),
            lanes: lanes::Lanes::default(),
            change: change::Change::default(),
            model_detections: Vec::new(),
            model_detection_time: -999.0,
            radar: radar::Radar::default(),
        }
    }
}

impl Projection {
    pub(crate) fn set_front_radar_only(&mut self, enabled: bool) {
        self.radar.front_only = enabled;
    }
    pub(crate) fn replay_front_only(&self) -> bool {
        self.radar.front_only
    }
    pub(crate) fn observe_replay_raw_corner(&mut self) {
        self.radar.corner_seen = true;
    }
    pub(crate) fn observe_replay_corner(
        &mut self,
        address: u32,
        detections: Vec<DetectedVehicle>,
        time: f64,
    ) {
        let index = usize::from(address == 0x1ea);
        self.radar.can_detections[index] = detections;
        self.radar.can_time[index] = time;
    }
    #[must_use]
    pub fn recorded_cutin_debug(&self) -> &str {
        &self.radar.debug_text
    }
    #[must_use]
    pub fn replay_geometry(&self) -> (LaneSnapshot, f64) {
        (
            self.lanes.snapshot(self.state.lane_width_m),
            self.change.recenter_start,
        )
    }
    pub fn set_onroad(&mut self, onroad: bool) {
        self.trip.set_onroad(onroad);
    }
    /// # Errors
    /// Returns malformed Cereal service or timestamp conversion errors.
    pub fn apply(&mut self, event: event::Reader<'_>) -> Result<(), Error> {
        let Reader::Struct(root) = event.into() else {
            unreachable!()
        };
        let Some(field) = root.which()? else {
            return Ok(());
        };
        let name = field
            .get_proto()
            .get_name()?
            .to_str()
            .map_err(|error| capnp::Error::failed(error.to_string()))?;
        let time = event.get_log_mono_time().to_f64().ok_or(Error::Clock)? / 1e9;
        self.apply_data(name, root.get(field)?, time, event.get_valid());
        Ok(())
    }
    pub fn apply_data(&mut self, name: &str, data: Reader<'_>, time: f64, valid: bool) {
        let value = Data(data);
        match name {
            "modelV2" => self.model(value, time),
            "drivingModelData" => self.driving_model(value),
            "controlsState" => self.controls(value),
            "selfdriveState" => {
                self.selfdrive(value);
                self.radar.sound(value, time);
            }
            "radarState" => self.radar.update(value, time, self.state.speed_kph),
            "liveTracks" => self.radar.tracks(value, time, self.state.speed_kph),
            "carControl" => self.state.lfa_active = value.field("latActive").boolean(),
            "longitudinalPlan" => self.longitudinal(value, valid),
            "lateralPlan" => self.lateral(value),
            "cameraOdometry" => self.odometry(value, valid),
            "liveCalibration" => self.calibration(value, valid),
            "deviceState" => self.device(value),
            "roadCameraState" => {
                self.state.camera_sensor = value
                    .field("sensor")
                    .text()
                    .map(|text| text.trim().to_lowercase());
            }
            "carParams" => self.car_params(value),
            "navInstruction" | "navInstructionCarrot" => self.navigation(value, time),
            _ => (),
        }
    }
}
