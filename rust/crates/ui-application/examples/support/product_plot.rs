use openpilot_ui_application::{
    context::Context,
    mici::onroad::debug_plot::{self, DebugPlot},
};
use openpilot_ui_framework::{geometry::Rect, widget::WidgetHandle};
use serde::Deserialize;
#[derive(Deserialize)]
pub struct Options {
    pub steps: Vec<Step>,
}
#[derive(Deserialize)]
pub struct Step {
    pub frame: u32,
    pub mode: i32,
    pub now: f64,
    pub messages: Vec<Vec<u8>>,
}
impl Options {
    pub fn step(&self, index: u32) -> Result<&Step, Box<dyn std::error::Error>> {
        self.steps
            .iter()
            .rev()
            .find(|step| step.frame <= index)
            .ok_or_else(|| "plot initial step missing".into())
    }
}
pub fn create(context: &Context, rect: Rect) -> WidgetHandle {
    WidgetHandle::new(DebugPlot::new(context.clone(), rect))
}
pub fn before(
    context: &Context,
    options: &Options,
    index: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let step = options.step(index)?;
    context
        .params
        .put("ShowPlotMode", step.mode.to_string().as_bytes())?;
    context
        .messages
        .borrow_mut()
        .state
        .update(f64::from(index) / 20.0, &step.messages)?;
    Ok(())
}
pub fn snapshot(
    context: &Context,
    widget: &WidgetHandle,
    mode: i32,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let plot = widget.get::<DebugPlot>()?;
    let messages = context.messages.borrow();
    let (values, title) = debug_plot::data::data(&messages.state, mode)?;
    Ok(
        serde_json::json!({"samples":plot.samples.snapshot(),"previous":plot.mode_previous(),"data":values,"title":title}),
    )
}
