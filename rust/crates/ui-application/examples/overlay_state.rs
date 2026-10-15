use openpilot_ui_application::{
    overlay_state::{Camera, Mode, State},
    params::Read,
    state::{ModelStatus, Status, UiState},
    Error,
};
use serde::Deserialize;
struct Params;
impl Read for Params {
    fn bytes(&self, _: &str) -> Result<Option<Vec<u8>>, Error> {
        Ok(None)
    }
    fn integer(&self, _: &str) -> Result<i32, Error> {
        Ok(0)
    }
    fn float(&self, _: &str) -> Result<f64, Error> {
        Ok(0.0)
    }
}
#[derive(Deserialize)]
struct Step {
    packets: Vec<Vec<u8>>,
    calibration: Option<Vec<u8>>,
    age: f64,
    started: bool,
    started_frame: i64,
    status: String,
    camera: u64,
    connected: bool,
    stream: i32,
    mode: Mode,
    mouse: bool,
    gpu: [bool; 3],
    badge: Option<(String, String)>,
    expected: serde_json::Value,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let steps: Vec<Step> = serde_json::from_reader(std::io::stdin())?;
    let mut ui = UiState::new(&Params, 0.0, ModelStatus::default())?;
    for (index, step) in steps.iter().enumerate() {
        let mut messages = openpilot_messaging::state::State::new(
            &[
                "carState",
                "selfdriveState",
                "longitudinalPlan",
                "carrotMan",
                "radarState",
                "modelV2",
                "liveCalibration",
            ],
            Default::default(),
        )?;
        messages.update(1.0, &step.packets)?;
        messages.update(
            1.0 + step.age,
            &step.calibration.iter().cloned().collect::<Vec<_>>(),
        )?;
        ui.started = step.started;
        ui.started_frame = step.started_frame;
        ui.status = match step.status.as_str() {
            "engaged" => Status::Engaged,
            "override" => Status::Override,
            _ => Status::Disengaged,
        };
        [
            ui.slow.usbgpu_active,
            ui.slow.usbgpu_loading,
            ui.slow.usbgpu_startup_failed,
        ] = step.gpu;
        ui.slow.recovery_badge = step
            .badge
            .as_ref()
            .map(|(text, style)| {
                serde_json::from_value(serde_json::json!({"text":text,"style":style}))
            })
            .transpose()?;
        let mut snapshot = State::default();
        snapshot.update(
            &ui,
            &messages,
            Camera {
                identity: step.camera,
                connected: step.connected,
                stream: step.stream,
            },
            step.mode.clone(),
            step.mouse,
        )?;
        let result = serde_json::json!({"key":snapshot.key.as_ref().unwrap().values(),"enabled":snapshot.enabled,"urgent":snapshot.urgent});
        if result != step.expected {
            return Err(format!(
                "source overlay state {index}: {result} != {}",
                step.expected
            )
            .into());
        }
    }
    println!(
        "PASS {} original OverlayState snapshots through actual cereal/state boundary",
        steps.len()
    );
    Ok(())
}
