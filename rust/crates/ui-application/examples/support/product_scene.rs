use super::{product_camera, product_driver, product_egpu, product_input};
use openpilot_startup_ui::config::Config;
use openpilot_ui_framework::geometry::Rect;
use serde::Deserialize;
use std::collections::BTreeMap;
#[derive(Deserialize)]
pub struct Scene {
    pub kind: String,
    pub driver: Option<product_driver::Options>,
    pub camera: Option<product_camera::Options>,
    pub alert: Option<super::product_alert::Options>,
    pub indicator: Option<super::product_indicator::Options>,
    pub vision: Option<super::product_vision::Options>,
    pub exp: Option<super::product_exp::Options>,
    pub hud: Option<super::product_hud::Options>,
    pub road: Option<super::product_road::Options>,
    pub root: Option<super::product_root::Options>,
    pub plot: Option<super::product_plot::Options>,
    pub config: Config,
    pub background: Option<[u8; 4]>,
    pub language: String,
    pub rect: Rect,
    pub frames: u32,
    pub prime: i32,
    #[serde(default)]
    pub params: BTreeMap<String, String>,
    #[serde(default)]
    pub raw_params: BTreeMap<String, Vec<u8>>,
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub network_type: u16,
    #[serde(default)]
    pub network_metered: bool,
    #[serde(default)]
    pub car: Option<openpilot_ui_application::state::CarConfig>,
    #[serde(default)]
    pub models: openpilot_ui_application::state::ModelStatus,
    #[serde(default)]
    pub steps: Vec<product_input::Step>,
    #[serde(default)]
    pub capture_effects: bool,
    #[serde(default)]
    pub dialog: Option<DialogProbe>,
    pub time_valid: Option<bool>,
    pub ssh_host: Option<String>,
    pub wifi: Option<openpilot_wifi::Snapshot>,
    pub egpu: Option<product_egpu::Options>,
    #[serde(default)]
    pub capture_frames: Vec<u32>,
}
#[derive(Deserialize)]
pub struct DialogProbe {
    pub title: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub red: bool,
    #[serde(default)]
    pub stay: bool,
}
