use openpilot_ui_application::{
    device::{Device, Input as DeviceInput},
    params::Read,
    state::{Input, ModelStatus, UiState},
    Error,
};
use serde::Deserialize;
use std::{cell::RefCell, collections::BTreeMap};
#[derive(Deserialize)]
struct Scene {
    big: bool,
    pc: bool,
    mici: bool,
    params: BTreeMap<String, Vec<u8>>,
    frames: Vec<Step>,
}
#[derive(Deserialize)]
struct Step {
    input: Input,
    #[serde(default)]
    params: BTreeMap<String, Option<Vec<u8>>>,
    #[serde(default)]
    failures: Vec<String>,
    #[serde(default)]
    models: ModelStatus,
    #[serde(default)]
    touch: bool,
    #[serde(default)]
    worker_busy: bool,
    #[serde(default)]
    override_timeout: Option<Option<i32>>,
    #[serde(default)]
    offroad_brightness: Option<Option<i32>>,
}
struct Parameters {
    values: BTreeMap<String, Vec<u8>>,
    failures: Vec<String>,
    calls: RefCell<Vec<String>>,
}
impl Read for Parameters {
    fn bytes(&self, key: &str) -> Result<Option<Vec<u8>>, Error> {
        self.calls.borrow_mut().push(key.into());
        if self.failures.iter().any(|v| v == key) {
            return Err(Error::Contract("fixture read failure"));
        }
        Ok(self.values.get(key).cloned())
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let scene: Scene = serde_json::from_reader(std::io::stdin())?;
    let mut params = Parameters {
        values: scene.params,
        failures: Vec::new(),
        calls: RefCell::default(),
    };
    let mut state = UiState::new(&params, 0.0, ModelStatus::default())?;
    let mut device = Device::new(openpilot_ui_application::device::Config {
        big: scene.big,
        pc: scene.pc,
        mici: scene.mici,
        target_fps: 20,
    });
    params.calls.borrow_mut().clear();
    let mut output = Vec::new();
    for step in scene.frames {
        for (key, value) in step.params {
            match value {
                Some(value) => {
                    params.values.insert(key, value);
                }
                None => {
                    params.values.remove(&key);
                }
            }
        }
        params.failures = step.failures;
        if let Some(value) = step.override_timeout {
            device.set_override_timeout(value, step.input.now, state.ignition);
        }
        if let Some(value) = step.offroad_brightness {
            device.set_offroad_brightness(value);
        }
        let transitions = state.update(&step.input, &params, step.models)?;
        let effects = device.update(
            &state,
            DeviceInput {
                now: step.input.now,
                left_down: step.touch,
                brightness_worker_busy: step.worker_busy,
                exposure_percent: step.input.exposure_percent,
            },
        );
        output.push(serde_json::json!({"state":state,"device":device,"transitions":transitions,"effects":effects,"reads":*params.calls.borrow()}));
        params.calls.borrow_mut().clear();
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
