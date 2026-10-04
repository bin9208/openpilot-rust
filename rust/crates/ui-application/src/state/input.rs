use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Panda {
    pub panda_type: u16,
    pub ignition_line: bool,
    pub ignition_can: bool,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ControlState {
    #[default]
    Disabled,
    PreEnabled,
    Enabled,
    SoftDisabling,
    Overriding,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Input {
    pub frame: i64,
    pub now: f64,
    pub fps: i32,
    pub panda_updated: bool,
    pub panda_receive_frame: i64,
    pub pandas: Vec<Panda>,
    pub wide_updated: bool,
    pub wide_alive: bool,
    pub wide_valid: bool,
    pub exposure_percent: f64,
    pub device_started: bool,
    pub selfdrive_updated: bool,
    pub enabled: bool,
    pub control_state: ControlState,
    pub lat_active: bool,
}
