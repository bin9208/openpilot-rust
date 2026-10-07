use capnp::{message::ReaderOptions, serialize};
use openpilot_cereal::{car_capnp::car_control::actuators::LongControlState, log_capnp::event};
use openpilot_plannerd::{
    car_state::CarState,
    car_state_decode,
    carrot::{self, Navigation},
    longitudinal_planner,
    model::Model,
    model_decode,
    radar::Radar,
    radar_decode,
    types::Personality,
};
use serde::Deserialize;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Deserialize)]
pub struct Checks {
    pub mode: bool,
    pub lane: bool,
    pub pose: bool,
    pub coasting: bool,
    pub navigation_seen: bool,
    pub navigation_valid: bool,
}
#[derive(Deserialize)]
pub struct Frame {
    pub time: f64,
    pub wall_time: f64,
    pub messages: BTreeMap<String, PathBuf>,
    pub parameters: BTreeMap<String, String>,
    pub checks: Checks,
}

pub struct Decoded {
    pub car: CarState,
    pub model: Model,
    radar: Radar,
    navigation: Option<Navigation>,
    pub curve_speed: f64,
    model_ns: u64,
    radar_ns: u64,
    pose_ns: u64,
    pose_valid: bool,
    yaw: f64,
    personality: Personality,
    enabled: bool,
    experimental: bool,
    control: LongControlState,
    force_deceleration: bool,
    desired_curvature: f64,
    pub curvature: f64,
}

impl Decoded {
    pub fn read(frame: &Frame) -> Result<Self, Box<dyn std::error::Error>> {
        let mut result = Self {
            car: CarState::default(),
            model: Model::default(),
            radar: Radar::default(),
            navigation: None,
            curve_speed: 0.,
            model_ns: 0,
            radar_ns: 0,
            pose_ns: 0,
            pose_valid: false,
            yaw: 0.,
            personality: Personality::Standard,
            enabled: false,
            experimental: false,
            control: LongControlState::Off,
            force_deceleration: false,
            desired_curvature: 0.,
            curvature: 0.,
        };
        for path in frame.messages.values() {
            let bytes = std::fs::read(path)?;
            let message = serialize::read_message(bytes.as_slice(), ReaderOptions::new())?;
            let event = message.get_root::<event::Reader<'_>>()?;
            match event.which()? {
                event::CarState(value) => result.car = car_state_decode::state(value?),
                event::ModelV2(value) => {
                    result.model = model_decode::model(value?)?;
                    result.model_ns = event.get_log_mono_time();
                }
                event::RadarState(value) => {
                    result.radar = radar_decode::radar(value?)?;
                    result.radar_ns = event.get_log_mono_time();
                }
                event::CarrotMan(value) => {
                    let value = value?;
                    result.curve_speed = f64::from(value.get_v_turn_speed());
                    if frame.checks.navigation_valid {
                        result.navigation = Some(Navigation {
                            atc_type: value.get_atc_type()?.to_str()?.into(),
                            traffic_state: value.get_traffic_state(),
                            active_carrot: value.get_active_carrot(),
                            x_dist_to_turn: f64::from(value.get_x_dist_to_turn()),
                            desired_speed: f64::from(value.get_desired_speed()),
                        });
                    }
                }
                event::LivePose(value) => {
                    let value = value?;
                    let angular = value.get_angular_velocity_device()?;
                    result.yaw = f64::from(angular.get_z());
                    result.pose_ns = event.get_log_mono_time();
                    result.pose_valid =
                        value.get_inputs_o_k() && value.get_sensors_o_k() && angular.get_valid();
                }
                event::SelfdriveState(value) => {
                    let value = value?;
                    result.enabled = value.get_enabled();
                    result.experimental = value.get_experimental_mode();
                    result.personality =
                        Personality::try_from(i32::from(u16::from(value.get_personality()?)))?;
                }
                event::ControlsState(value) => {
                    let value = value?;
                    result.control = value.get_long_control_state()?;
                    result.force_deceleration = value.get_force_decel();
                    result.desired_curvature = f64::from(value.get_desired_curvature());
                    result.curvature = f64::from(value.get_curvature());
                }
                _ => {}
            }
        }
        Ok(result)
    }

    pub fn input<'a>(&'a self, frame: &Frame) -> longitudinal_planner::Input<'a> {
        longitudinal_planner::Input {
            carrot: carrot::Input {
                car: &self.car,
                model: &self.model,
                radar: &self.radar,
                personality: self.personality,
                navigation: self.navigation.as_ref(),
                navigation_receive_time: None,
                mode_checks: frame.checks.mode,
                lane_valid: frame.checks.lane,
                model_ns: self.model_ns,
                radar_ns: self.radar_ns,
                pose_ns: self.pose_ns,
                pose_valid: frame.checks.pose && self.pose_valid,
                yaw_rate: self.yaw,
            },
            enabled: self.enabled,
            experimental: self.experimental,
            control_state: self.control,
            force_deceleration: self.force_deceleration,
            desired_curvature: self.desired_curvature,
            curvature: self.curvature,
            coasting_checks: frame.checks.coasting,
            navigation_seen: frame.checks.navigation_seen,
        }
    }
}
