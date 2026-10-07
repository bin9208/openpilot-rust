use openpilot_ui_application::{context::Context, onroad::exp_button::ExpButton, params::Read};
use openpilot_ui_framework::{canvas::Canvas, widget::WidgetHandle};
use serde::Deserialize;
#[derive(Deserialize)]
pub struct Options {
    pub button_size: i32,
    pub icon_size: i32,
    pub steps: Vec<Step>,
}
#[derive(Deserialize)]
pub struct Step {
    pub frame: u32,
    pub experimental: bool,
    pub engageable: bool,
    pub enabled: bool,
}
pub fn create(
    context: &Context,
    canvas: &mut Canvas,
    options: &Options,
) -> Result<WidgetHandle, Box<dyn std::error::Error>> {
    Ok(WidgetHandle::new(ExpButton::new(
        context.clone(),
        canvas,
        options.button_size,
        options.icon_size,
    )?))
}
pub fn before(
    context: &Context,
    options: &Options,
    index: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let step = options
        .steps
        .iter()
        .rev()
        .find(|step| step.frame <= index)
        .ok_or("exp initial step missing")?;
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<openpilot_cereal::log_capnp::event::Builder>();
    event.set_valid(true);
    let mut data = event.init_selfdrive_state();
    data.set_experimental_mode(step.experimental);
    data.set_engageable(step.engageable);
    data.set_enabled(step.enabled);
    context.messages.borrow_mut().state.update(
        f64::from(index) / 20.0,
        &[capnp::serialize::write_message_to_words(&message)],
    )?;
    Ok(())
}
pub fn snapshot(
    context: &Context,
    widget: &WidgetHandle,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let widget = widget.get::<ExpButton>()?;
    Ok(
        serde_json::json!({"actual":widget.experimental(),"held":widget.held_mode(),"end":widget.hold_end(),
                         "param":context.params.bytes("ExperimentalMode")?.map(String::from_utf8).transpose()?,"pressed":openpilot_ui_framework::widget::Widget::state(&*widget).is_pressed()}),
    )
}
