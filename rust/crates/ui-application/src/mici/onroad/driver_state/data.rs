use crate::{context::Context, state::messages};
use openpilot_cereal::log_capnp::{
    driver_monitoring_state::MonitoringPolicy, selfdrive_state::AlertSize,
};
#[derive(Default)]
pub struct Data {
    pub rhd: bool,
    pub active: bool,
    pub detected: bool,
    pub awareness: i8,
    pub orientation: Vec<f64>,
    pub deviation: Vec<f64>,
    pub position: Vec<f64>,
    pub eyes: [f64; 2],
    pub glasses: f64,
    pub drawable: bool,
}
impl Data {
    pub fn read(context: &Context) -> Result<Self, crate::Error> {
        let messages = context.messages.borrow();
        let state = &messages.state;
        let dm = messages::driver_monitoring_state(state)?;
        let vision = dm.get_vision_policy_state()?;
        let drivers = messages::driver_state(state)?;
        let driver = if dm.get_is_r_h_d() {
            drivers.get_right_driver_data()?
        } else {
            drivers.get_left_driver_data()?
        };
        Ok(Self {
            rhd: dm.get_is_r_h_d(),
            active: dm.get_active_policy()? == MonitoringPolicy::Vision,
            detected: vision.get_face_detected(),
            awareness: vision.get_awareness_percent(),
            orientation: driver
                .get_face_orientation()?
                .iter()
                .map(f64::from)
                .collect(),
            deviation: driver
                .get_face_orientation_std()?
                .iter()
                .map(f64::from)
                .collect(),
            position: driver.get_face_position()?.iter().map(f64::from).collect(),
            eyes: [
                f64::from(driver.get_left_eye_prob()),
                f64::from(driver.get_right_eye_prob()),
            ],
            glasses: f64::from(driver.get_sunglasses_prob()),
            drawable: messages::selfdrive_state(state)?.get_alert_size()? == AlertSize::None
                && state.topic("driverStateV2")?.receive_frame > context.ui.borrow().started_frame,
        })
    }
}
