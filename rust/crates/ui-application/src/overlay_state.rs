//! One final-source invalidation snapshot shared by the model and HUD layers.
use crate::{
    state::{messages, Status, UiState},
    Error,
};
use openpilot_messaging::state::State as Messages;
use std::{cell::RefCell, rc::Rc};
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum Mode {
    Large(bool),
    Compact((bool, i32)),
}
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub identity: u64,
    pub connected: bool,
    pub stream: i32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Key {
    started_frame: i64,
    status: Status,
    camera: u64,
    connected: bool,
    stream: i32,
    mode: Mode,
    enabled: bool,
    experimental: bool,
    cruise: f64,
    gear: u16,
    pedals: [bool; 3],
    soft_hold: i16,
    carrot_cruise: i16,
    x_state: i32,
    traffic: i32,
    throttle: bool,
    carrot_traffic: i32,
    suspended: bool,
    observation: u8,
    assist: u8,
    display: (u8, bool, bool),
    lead_one: bool,
    track: i32,
    lead_two: bool,
    calibration: i64,
    health: [bool; 10],
    gpu: [bool; 3],
    recovery: Option<(String, String)>,
}
impl Key {
    /// Source tuple order, also used by the real cereal boundary regression.
    pub fn values(&self) -> serde_json::Value {
        serde_json::json!([
            self.started_frame,
            self.status,
            self.camera,
            self.connected,
            self.stream,
            self.mode,
            self.enabled,
            self.experimental,
            self.cruise,
            self.gear,
            self.pedals[0],
            self.pedals[1],
            self.pedals[2],
            self.soft_hold,
            self.carrot_cruise,
            self.x_state,
            self.traffic,
            self.throttle,
            self.carrot_traffic,
            self.suspended,
            self.observation,
            self.assist,
            self.display,
            self.lead_one,
            self.track,
            self.lead_two,
            self.calibration,
            self.health[0],
            self.health[1],
            self.health[2],
            self.health[3],
            self.health[4],
            self.health[5],
            self.health[6],
            self.health[7],
            self.health[8],
            self.health[9],
            self.gpu[0],
            self.gpu[1],
            self.gpu[2],
            self.recovery
        ])
    }
}
#[derive(Clone, Debug, Default)]
pub struct State {
    pub key: Option<Key>,
    pub enabled: bool,
    pub urgent: bool,
}
pub type Shared = Rc<RefCell<State>>;
fn raw<T: Into<u16>>(value: Result<T, capnp::NotInSchema>) -> u16 {
    value.map(Into::into).unwrap_or_else(|e| e.0)
}
impl State {
    pub fn update(
        &mut self,
        ui: &UiState,
        sm: &Messages,
        camera: Camera,
        mode: Mode,
        mouse_events: bool,
    ) -> Result<(), Error> {
        let cs = messages::car_state(sm)?;
        let ss = messages::selfdrive_state(sm)?;
        let lp = messages::longitudinal_plan(sm)?;
        let cm = messages::carrot_man(sm)?;
        let radar = messages::radar_state(sm)?;
        self.enabled = ui.started;
        self.urgent = raw(ss.get_alert_size()) != 0
            || raw(ss.get_alert_hud_visual()) != 0
            || cs.get_left_blindspot()
            || cs.get_right_blindspot()
            || raw(messages::model(sm)?.get_meta()?.get_lane_change_state()) != 0
            || mouse_events;
        let mut health = [false; 10];
        for (index, name) in [
            "modelV2",
            "carState",
            "radarState",
            "longitudinalPlan",
            "carrotMan",
        ]
        .into_iter()
        .enumerate()
        {
            let topic = sm.topic(name)?;
            health[index * 2] = topic.valid;
            health[index * 2 + 1] = topic.alive;
        }
        self.key = Some(Key {
            started_frame: ui.started_frame,
            status: ui.status,
            camera: camera.identity,
            connected: camera.connected,
            stream: camera.stream,
            mode,
            enabled: ss.get_enabled(),
            experimental: ss.get_experimental_mode(),
            cruise: f64::from(cs.get_v_cruise_cluster()),
            gear: raw(cs.get_gear_shifter()),
            pedals: [
                cs.get_brake_pressed(),
                cs.get_gas_pressed(),
                cs.get_brake_hold_active(),
            ],
            soft_hold: cs.get_soft_hold_active(),
            carrot_cruise: cs.get_carrot_cruise(),
            x_state: lp.get_x_state(),
            traffic: lp.get_traffic_state(),
            throttle: lp.get_allow_throttle(),
            carrot_traffic: cm.get_traffic_state(),
            suspended: lp.get_signal_detection_suspended(),
            observation: lp.get_signal_observation_color(),
            assist: lp.get_signal_assist_action(),
            display: crate::signal_display::signal_display_status(),
            lead_one: radar.get_lead_one()?.get_status(),
            track: radar.get_lead_one()?.get_radar_track_id(),
            lead_two: radar.get_lead_two()?.get_status(),
            calibration: sm.topic("liveCalibration")?.receive_frame,
            health,
            gpu: [
                ui.slow.usbgpu_active,
                ui.slow.usbgpu_loading,
                ui.slow.usbgpu_startup_failed,
            ],
            recovery: ui
                .slow
                .recovery_badge
                .as_ref()
                .map(|b| (b.text.clone(), b.style.clone())),
        });
        Ok(())
    }
    pub fn control(&self, index: u64) -> openpilot_ui_framework::overlay::Control {
        openpilot_ui_framework::overlay::Control {
            index: index as i64,
            enabled: self.enabled,
            urgent: self.urgent,
        }
    }
}
