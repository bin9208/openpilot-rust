use openpilot_ui_application::{context::Context, mici::onroad::vision_renderer::VisionRenderer};
use openpilot_ui_framework::widget::WidgetHandle;
use serde::Deserialize;
use std::{cell::Cell, rc::Rc};
#[derive(Deserialize)]
pub struct Options {
    pub steps: Vec<Step>,
    #[serde(skip)]
    clock: Rc<Cell<i128>>,
}
#[derive(Deserialize)]
pub struct Step {
    pub frame: u32,
    pub started: bool,
    pub share: bool,
    #[serde(default)]
    pub started_frame: i64,
    #[serde(default)]
    pub left: bool,
    #[serde(default)]
    pub right: bool,
    pub car_publish: bool,
    pub car_valid: bool,
    pub vision_publish: bool,
    pub vision_valid: bool,
    pub payload: Option<Vec<u8>>,
    pub now: Option<i128>,
}
pub fn create(context: &Context, options: &Options) -> WidgetHandle {
    let clock = options.clock.clone();
    WidgetHandle::new(VisionRenderer::with_clock(
        context.clone(),
        Rc::new(move || clock.get()),
    ))
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
        .ok_or("vision initial step missing")?;
    options.clock.set(
        step.now
            .map_or(10_000_000_000 + i128::from(index) * 50_000_000, |now| {
                now + i128::from(index - step.frame) * 50_000_000
            }),
    );
    {
        let mut ui = context.ui.borrow_mut();
        ui.started = step.started;
        ui.slow.share_data = step.share;
        ui.started_frame = step.started_frame;
    }
    let mut messages = Vec::new();
    if step.car_publish {
        let mut message = capnp::message::Builder::new_default();
        let mut event = message.init_root::<openpilot_cereal::log_capnp::event::Builder>();
        event.set_valid(step.car_valid);
        let mut car = event.init_car_state();
        car.set_left_blindspot(step.left);
        car.set_right_blindspot(step.right);
        messages.push(capnp::serialize::write_message_to_words(&message));
    }
    if step.vision_publish {
        let mut message = capnp::message::Builder::new_default();
        let mut event = message.init_root::<openpilot_cereal::log_capnp::event::Builder>();
        event.set_valid(step.vision_valid);
        event.set_custom_reserved_raw_data0(step.payload.as_deref().unwrap_or_default());
        messages.push(capnp::serialize::write_message_to_words(&message));
    }
    context
        .messages
        .borrow_mut()
        .state
        .update(f64::from(index) / 20.0, &messages)?;
    Ok(())
}
pub fn snapshot(
    context: &Context,
    widget: &WidgetHandle,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let messages = context.messages.borrow();
    let topic = messages.state.topic("carState")?;
    let ui = context.ui.borrow();
    Ok(
        serde_json::json!({"state":widget.get::<VisionRenderer>()?.display_state(),
                         "car_fresh":topic.valid && topic.alive && topic.receive_frame >= ui.started_frame}),
    )
}
