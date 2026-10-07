//! Port of openpilot/tools/joystick/joystickd.py; original MIT license applies.
use crate::Error;
use openpilot_control_policy::{
    math::{clip, maximum},
    vehicle::{Physical, VehicleModel},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize)]
pub struct Config {
    pub physical: Physical,
    pub stopping_speed: f64,
    pub openpilot_longitudinal: bool,
    pub pcm_cruise: bool,
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct Input {
    pub enabled: bool,
    pub active: bool,
    pub steer_fault_temporary: bool,
    pub steer_fault_permanent: bool,
    pub override_longitudinal: bool,
    pub cruise_enabled: bool,
    pub speed: f64,
    pub steering_angle: f64,
    pub roll: f64,
    pub angle_offset: f64,
    pub frame: i64,
    pub joystick_frame: i64,
    pub axes: Vec<f64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LongState {
    #[default]
    Off,
    Pid,
    Stopping,
}

#[derive(Default)]
pub struct Command {
    pub enabled: bool,
    pub lat_active: bool,
    pub long_active: bool,
    pub cancel: bool,
    pub resume: bool,
    pub long_state: LongState,
    pub accel: f32,
    pub torque: f32,
    pub steering_angle: f32,
    pub curvature: f32,
}

pub struct Controller {
    config: Config,
    vehicle: VehicleModel,
}

impl Controller {
    pub fn new(config: Config) -> Self {
        let vehicle = VehicleModel::new(config.physical.clone());
        Self { config, vehicle }
    }

    pub fn control(&self, input: &Input) -> Result<Command, Error> {
        let mut command = Command {
            enabled: input.enabled,
            lat_active: input.active
                && !input.steer_fault_temporary
                && !input.steer_fault_permanent,
            long_active: input.enabled
                && !input.override_longitudinal
                && self.config.openpilot_longitudinal,
            cancel: input.cruise_enabled && (!input.enabled || !self.config.pcm_cruise),
            ..Command::default()
        };
        let reset =
            input.joystick_frame == 0 || input.frame.saturating_sub(input.joystick_frame) > 20;
        if command.long_active {
            let axis = if reset {
                0.0
            } else {
                *input
                    .axes
                    .first()
                    .ok_or(Error::Contract("missing longitudinal joystick axis"))?
            };
            // Cereal Float32 assignments round before later source calculations.
            command.accel = (4.0 * clip(axis, -1.0, 1.0)) as f32;
            command.long_state = if input.speed > self.config.stopping_speed {
                LongState::Pid
            } else {
                LongState::Stopping
            };
            command.resume = command.accel > 0.0;
        }
        if command.lat_active {
            let max_curvature = 3.0 / maximum(input.speed.powi(2), 5.0);
            let max_angle = self
                .vehicle
                .steer(max_curvature, input.speed, input.roll)?
                .to_degrees();
            let axis = if reset {
                0.0
            } else {
                *input
                    .axes
                    .get(1)
                    .ok_or(Error::Contract("missing lateral joystick axis"))?
            };
            command.torque = clip(axis, -1.0, 1.0) as f32;
            command.steering_angle = (f64::from(command.torque) * max_angle) as f32;
            command.curvature = (f64::from(command.torque) * -max_curvature) as f32;
        }
        Ok(command)
    }

    // Source publishes carControl before calculating measured curvature, which can fail.
    pub fn curvature(&self, input: &Input) -> Result<f32, Error> {
        Ok(-self.vehicle.curvature(
            (input.steering_angle - input.angle_offset).to_radians(),
            input.speed,
            input.roll,
        )? as f32)
    }
}
