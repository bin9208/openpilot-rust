use openpilot_ui_application::{
    context::Context,
    mici::onroad::{confidence_ball::ConfidenceBall, traffic_light::TrafficLight},
    state::Status,
};
use openpilot_ui_framework::{canvas::Canvas, widget::WidgetHandle};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Options {
    pub demo: bool,
    pub steps: Vec<Step>,
}
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StatusInput {
    Disengaged,
    Engaged,
    Override,
}
#[derive(Deserialize)]
pub struct Step {
    pub frame: u32,
    pub status: StatusInput,
    pub traffic: i32,
    pub brake: Vec<f32>,
    pub steer: Vec<f32>,
    pub value: Option<f64>,
}
pub fn create(
    context: &Context,
    _: &mut Canvas,
    kind: &str,
    options: &Options,
) -> Result<WidgetHandle, Box<dyn std::error::Error>> {
    Ok(match kind {
        "confidence" => WidgetHandle::new(ConfidenceBall::new(context.clone(), 20.0, options.demo)),
        "traffic" => WidgetHandle::new(TrafficLight::new(context.clone())),
        _ => return Err("invalid indicator fixture kind".into()),
    })
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
        .ok_or("indicator initial step missing")?;
    context.ui.borrow_mut().status = match step.status {
        StatusInput::Disengaged => Status::Disengaged,
        StatusInput::Engaged => Status::Engaged,
        StatusInput::Override => Status::Override,
    };
    let mut model = capnp::message::Builder::new_default();
    let mut predictions = model
        .init_root::<openpilot_cereal::log_capnp::event::Builder>()
        .init_model_v2()
        .init_meta()
        .init_disengage_predictions();
    let mut brake = predictions
        .reborrow()
        .init_brake_disengage_probs(u32::try_from(step.brake.len())?);
    for (i, value) in step.brake.iter().enumerate() {
        brake.set(u32::try_from(i)?, *value);
    }
    let mut steer = predictions.init_steer_override_probs(u32::try_from(step.steer.len())?);
    for (i, value) in step.steer.iter().enumerate() {
        steer.set(u32::try_from(i)?, *value);
    }
    let mut plan = capnp::message::Builder::new_default();
    plan.init_root::<openpilot_cereal::log_capnp::event::Builder>()
        .init_longitudinal_plan()
        .set_traffic_state(step.traffic);
    context.messages.borrow_mut().state.update(
        f64::from(index) / 20.0,
        &[
            capnp::serialize::write_message_to_words(&model),
            capnp::serialize::write_message_to_words(&plan),
        ],
    )?;
    if let Some(value) = step.value {
        widget.get_mut::<ConfidenceBall>()?.update_filter(value);
    }
    Ok(())
}
pub fn snapshot(
    widget: &WidgetHandle,
    kind: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    Ok(match kind {
        "confidence" => serde_json::json!({"value":widget.get::<ConfidenceBall>()?.value()}),
        "traffic" => {
            let widget = widget.get::<TrafficLight>()?;
            serde_json::json!({"value":widget.opacity(),"visible":widget.visible()})
        }
        _ => return Err("invalid indicator fixture kind".into()),
    })
}
