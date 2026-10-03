use openpilot_ui_application::{
    context::Context,
    mici::onroad::{
        confidence_ball::ConfidenceBall, torque_bar::TorqueBar, traffic_light::TrafficLight,
    },
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
    #[serde(default)]
    pub angle: bool,
    #[serde(default)]
    pub lat_active: bool,
    #[serde(default)]
    pub speed: f32,
    #[serde(default)]
    pub curvature: f32,
    #[serde(default)]
    pub desired: f32,
    #[serde(default)]
    pub roll: f32,
    #[serde(default)]
    pub torque: f32,
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
        "torque" => WidgetHandle::new(TorqueBar::new(context.clone(), 20.0, options.demo)),
        _ => return Err("invalid indicator fixture kind".into()),
    })
}
pub fn before(
    context: &Context,
    widget: &WidgetHandle,
    kind: &str,
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
    if kind == "torque" {
        let mut messages = Vec::new();
        let mut controls = capnp::message::Builder::new_default();
        let mut data = controls
            .init_root::<openpilot_cereal::log_capnp::event::Builder>()
            .init_controls_state();
        data.set_curvature(step.curvature);
        data.set_desired_curvature(step.desired);
        if step.angle {
            data.init_lateral_control_state().init_angle_state();
        } else {
            data.init_lateral_control_state().init_torque_state();
        }
        messages.push(capnp::serialize::write_message_to_words(&controls));
        let mut car = capnp::message::Builder::new_default();
        car.init_root::<openpilot_cereal::log_capnp::event::Builder>()
            .init_car_state()
            .set_v_ego(step.speed);
        messages.push(capnp::serialize::write_message_to_words(&car));
        let mut control = capnp::message::Builder::new_default();
        control
            .init_root::<openpilot_cereal::log_capnp::event::Builder>()
            .init_car_control()
            .set_lat_active(step.lat_active);
        messages.push(capnp::serialize::write_message_to_words(&control));
        let mut parameters = capnp::message::Builder::new_default();
        parameters
            .init_root::<openpilot_cereal::log_capnp::event::Builder>()
            .init_live_parameters()
            .set_roll(step.roll);
        messages.push(capnp::serialize::write_message_to_words(&parameters));
        let mut output = capnp::message::Builder::new_default();
        output
            .init_root::<openpilot_cereal::log_capnp::event::Builder>()
            .init_car_output()
            .init_actuators_output()
            .set_torque(step.torque);
        messages.push(capnp::serialize::write_message_to_words(&output));
        context
            .messages
            .borrow_mut()
            .state
            .update(f64::from(index) / 20.0, &messages)?;
        if let Some(value) = step.value {
            widget.get_mut::<TorqueBar>()?.update_filter(value);
        }
        return Ok(());
    }
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
        "torque" => {
            let widget = widget.get::<TorqueBar>()?;
            serde_json::json!({"value":widget.value(),"opacity":widget.opacity()})
        }
        _ => return Err("invalid indicator fixture kind".into()),
    })
}
