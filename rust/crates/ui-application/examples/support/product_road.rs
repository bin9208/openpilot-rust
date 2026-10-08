use openpilot_ui_application::{context::Context, onroad::augmented::Road, state::Status};
use openpilot_ui_framework::widget::WidgetHandle;
use serde::Deserialize;
use std::collections::BTreeMap;
#[derive(Deserialize)]
pub struct Options {
    steps: Vec<Step>,
}
#[derive(Deserialize)]
struct Step {
    frame: u32,
    messages: Vec<Vec<u8>>,
    started: bool,
    #[serde(default)]
    status: u8,
    #[serde(default)]
    started_time: f64,
    #[serde(default)]
    started_frame: i64,
    #[serde(default)]
    params: BTreeMap<String, String>,
    #[serde(default)]
    memory: BTreeMap<String, String>,
    #[serde(default)]
    suppress: bool,
    #[serde(default)]
    lat_active: bool,
}
pub fn before(
    context: &Context,
    widget: &WidgetHandle,
    options: &Options,
    index: u32,
    root: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let step = options
        .steps
        .iter()
        .rev()
        .find(|step| step.frame <= index)
        .ok_or("initial road step missing")?;
    for (key, value) in &step.params {
        context.params.put(key, value.as_bytes())?;
    }
    for (key, value) in &step.memory {
        context.memory.put(key, value.as_bytes())?;
    }
    context.refresh_params()?;
    {
        let mut ui = context.ui.borrow_mut();
        ui.started = step.started;
        ui.ignition = step.started;
        ui.panda_type = 1;
        ui.status = match step.status {
            0 => Status::Disengaged,
            1 => Status::Engaged,
            2 => Status::Override,
            _ => return Err("invalid road status".into()),
        };
        ui.engaged = step.status == 1;
        ui.lat_active = step.lat_active;
        ui.started_frame = step.started_frame;
        ui.started_time = step.started_time;
    }
    context
        .messages
        .borrow_mut()
        .state
        .update(f64::from(index) / 20.0, &step.messages)?;
    if !root {
        widget
            .get_mut::<Road>()?
            .set_cluster_hud_connected(step.suppress, false);
    }
    Ok(())
}
