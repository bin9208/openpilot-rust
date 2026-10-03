mod catalog;
mod diagnostics;
mod finish;
use capnp::message::{Builder, HeapAllocator};
pub(crate) use catalog::speed_gain;
pub use catalog::{platform, Platform};
pub use diagnostics::{parameter_diagnostics, DiagnosticInput};
pub(crate) use finish::bytes_repr;
pub use finish::{finish, FinishOptions};
use openpilot_cereal::car_capnp::car_params;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unknown vehicle platform: {0}")]
    UnknownPlatform(String),
    #[error("source torque catalog has no entry for {0}")]
    MissingTorque(String),
    #[error(transparent)]
    Catalog(#[from] serde_json::Error),
    #[error(transparent)]
    Schema(#[from] capnp::Error),
    #[error(transparent)]
    Enum(#[from] capnp::NotInSchema),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
}

pub fn baseline(candidate: &str) -> Result<Builder<HeapAllocator>, Error> {
    let platform = platform(candidate)?;
    let torque = platform.torque()?;
    let mut message = Builder::new_default();
    let mut cp = message.init_root::<car_params::Builder>();
    cp.set_car_fingerprint(candidate);
    cp.set_max_lateral_accel(torque.max_lateral_accel as f32);
    cp.set_auto_resume_sng(true);
    cp.set_steer_control_type(car_params::SteerControlType::Torque);
    cp.set_wheel_speed_factor(1.);
    cp.set_pcm_cruise(true);
    cp.set_steer_ratio_rear(0.);
    cp.set_openpilot_longitudinal_control(false);
    cp.set_stop_accel(-2.);
    cp.set_stopping_decel_rate(0.8);
    cp.set_v_ego_stopping(0.5);
    cp.set_v_ego_starting(0.5);
    let mut long = cp.reborrow().init_longitudinal_tuning();
    long.set_kf(1.);
    long.reborrow().init_kp_b_p(1).set(0, 0.);
    long.reborrow().init_kp_v(1).set(0, 0.);
    long.reborrow().init_ki_b_p(1).set(0, 0.);
    long.reborrow().init_ki_v(1).set(0, 0.);
    cp.set_longitudinal_actuator_delay(0.15);
    cp.set_steer_limit_timer(1.);
    cp.set_mass(platform.mass as f32);
    cp.set_wheelbase(platform.wheelbase as f32);
    cp.set_steer_ratio(platform.steer_ratio as f32);
    let wheelbase = f64::from(cp.reborrow_as_reader().get_wheelbase());
    cp.set_center_to_front((wheelbase * platform.center_front_ratio) as f32);
    cp.set_min_enable_speed(platform.min_enable_speed as f32);
    cp.set_min_steer_speed(platform.min_steer_speed as f32);
    cp.set_tire_stiffness_factor(platform.tire_stiffness_factor as f32);
    cp.set_flags(platform.flags);
    Ok(message)
}

#[derive(Clone, Copy, serde::Deserialize)]
pub struct TorqueOptions {
    pub deadzone_deg: f64,
    pub use_steering_angle: bool,
}
impl Default for TorqueOptions {
    fn default() -> Self {
        Self {
            deadzone_deg: 0.,
            use_steering_angle: true,
        }
    }
}

pub fn configure_torque(
    candidate: &str,
    tuning: car_params::lateral_tuning::Builder<'_>,
    options: TorqueOptions,
) -> Result<(), Error> {
    let platform = platform(candidate)?;
    let values = platform.torque()?;
    let mut torque = tuning.init_torque();
    torque.set_use_steering_angle(options.use_steering_angle);
    torque.set_kp(1.);
    torque.set_kf(1.);
    torque.set_ki(0.1);
    torque.set_friction(values.friction.unwrap_or(f64::NAN) as f32);
    torque.set_lat_accel_factor(values.lat_accel_factor.unwrap_or(f64::NAN) as f32);
    torque.set_lat_accel_offset(0.);
    torque.set_steering_angle_deadzone_deg(options.deadzone_deg as f32);
    Ok(())
}
