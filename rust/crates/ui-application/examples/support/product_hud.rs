use openpilot_ui_application::{context::Context, mici::onroad::hud::Hud};
use openpilot_ui_framework::{canvas::Canvas, widget::WidgetHandle};
use serde::Deserialize;
use std::collections::BTreeMap;
#[derive(Deserialize)]
pub struct Options {
    pub steps: Vec<Step>,
    #[serde(default)]
    pub debug_speed: bool,
    #[serde(default)]
    pub debug_traffic: bool,
}
#[derive(Deserialize)]
pub struct Step {
    pub frame: u32,
    pub messages: Vec<Vec<u8>>,
    #[serde(default)]
    pub started_frame: i64,
    #[serde(default)]
    pub critical: bool,
    #[serde(default)]
    pub top_icons: bool,
    #[serde(default)]
    pub lat_active: bool,
    #[serde(default)]
    pub params: BTreeMap<String, String>,
    #[serde(default)]
    pub memory: BTreeMap<String, String>,
}
pub fn create(
    context: &Context,
    canvas: &mut Canvas,
    options: &Options,
) -> Result<WidgetHandle, Box<dyn std::error::Error>> {
    if context.big {
        let mut hud = openpilot_ui_application::onroad::hud::Hud::new(context.clone(), canvas)?;
        hud.set_debug(options.debug_speed);
        return Ok(WidgetHandle::new(hud));
    }
    let mut hud = Hud::new(context.clone(), canvas, 20.0)?;
    hud.set_debug(options.debug_speed, options.debug_traffic);
    Ok(WidgetHandle::new(hud))
}
pub fn before(
    context: &Context,
    widget: &WidgetHandle,
    options: &Options,
    index: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let step = options
        .steps
        .iter()
        .rev()
        .find(|step| step.frame <= index)
        .ok_or("HUD initial step missing")?;
    for (key, value) in &step.params {
        context.params.put(key, value.as_bytes())?;
    }
    for (key, value) in &step.memory {
        context.memory.put(key, value.as_bytes())?;
    }
    context.refresh_params()?;
    {
        let mut ui = context.ui.borrow_mut();
        ui.started_frame = step.started_frame;
        ui.lat_active = step.lat_active;
    }
    if !context.big {
        let mut hud = widget.get_mut::<Hud>()?;
        hud.set_wheel_critical_icon(step.critical);
        hud.set_can_draw_top_icons(step.top_icons);
    }
    context
        .messages
        .borrow_mut()
        .state
        .update(f64::from(index) / 20.0, &step.messages)?;
    Ok(())
}
pub fn snapshot(widget: &WidgetHandle) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    if let Ok(widget) = widget.get::<openpilot_ui_application::onroad::hud::Hud>() {
        return Ok(serde_json::to_value(widget.snapshot())?);
    }
    Ok(serde_json::to_value(widget.get::<Hud>()?.snapshot())?)
}
