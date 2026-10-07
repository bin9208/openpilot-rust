use openpilot_ui_application::{context::Context, onroad::alert::Alert};
use openpilot_ui_framework::{
    canvas::Canvas,
    widget::{RenderResult, WidgetHandle},
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Options {
    pub steps: Vec<Step>,
    pub initial: Option<Alert>,
    #[serde(default)]
    pub started_frame: i64,
}

#[derive(Deserialize)]
pub struct Step {
    pub frame: u32,
    pub alert: Alert,
    #[serde(default)]
    pub left: bool,
    #[serde(default)]
    pub right: bool,
    #[serde(default = "publish")]
    pub publish: bool,
    pub now: Option<f64>,
}
fn publish() -> bool {
    true
}
impl Options {
    pub fn now(&self, index: u32) -> f64 {
        self.steps
            .iter()
            .rev()
            .find(|step| step.frame <= index)
            .and_then(|step| {
                step.now
                    .map(|now| now + f64::from(index - step.frame) / 20.0)
            })
            .unwrap_or(f64::from(index) / 20.0)
    }
}

pub fn create(
    context: &Context,
    canvas: &mut Canvas,
    big: bool,
    options: &Options,
) -> Result<WidgetHandle, Box<dyn std::error::Error>> {
    context.ui.borrow_mut().started_frame = options.started_frame;
    if let Some(initial) = &options.initial {
        context
            .messages
            .borrow_mut()
            .state
            .update(0.0, &[message(initial)?])?;
    }
    Ok(if big {
        WidgetHandle::new(openpilot_ui_application::onroad::alert::Alerts::new(
            context.clone(),
        ))
    } else {
        WidgetHandle::new(openpilot_ui_application::mici::onroad::alert::Alerts::new(
            context.clone(),
            canvas,
            20.0,
        )?)
    })
}

pub fn before(
    context: &Context,
    options: &Options,
    index: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(step) = options.steps.iter().rev().find(|step| step.frame <= index) else {
        return Err("alert fixture has no initial step".into());
    };
    let mut car = capnp::message::Builder::new_default();
    let mut cs = car
        .init_root::<openpilot_cereal::log_capnp::event::Builder>()
        .init_car_state();
    cs.set_left_blinker(step.left);
    cs.set_right_blinker(step.right);
    let mut messages = vec![capnp::serialize::write_message_to_words(&car)];
    if step.publish {
        messages.push(message(&step.alert)?);
    }
    context
        .messages
        .borrow_mut()
        .state
        .update(options.now(index), &messages)?;
    Ok(())
}
fn message(alert: &Alert) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut state = capnp::message::Builder::new_default();
    let mut ss = state
        .init_root::<openpilot_cereal::log_capnp::event::Builder>()
        .init_selfdrive_state();
    ss.set_alert_text1(alert.text1.as_str());
    ss.set_alert_text2(alert.text2.as_str());
    ss.set_alert_size(alert.size.try_into()?);
    ss.set_alert_status(alert.status.try_into()?);
    ss.set_alert_hud_visual(alert.visual_alert.try_into()?);
    ss.set_alert_type(alert.alert_type.as_str());
    Ok(capnp::serialize::write_message_to_words(&state))
}

pub fn snapshot(
    widget: &WidgetHandle,
    big: bool,
    rendered: RenderResult,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let current = if big {
        widget
            .get::<openpilot_ui_application::onroad::alert::Alerts>()?
            .current()?
    } else {
        widget
            .get_mut::<openpilot_ui_application::mici::onroad::alert::Alerts>()?
            .current()?
    };
    let mut current = serde_json::to_value(current)?;
    if big {
        if let Some(alert) = current.as_object_mut() {
            alert.remove("visual_alert");
            alert.remove("alert_type");
        }
    }
    let rendered = match rendered {
        RenderResult::None => None,
        RenderResult::Bool(value) => Some(value),
        RenderResult::Dialog(_) | RenderResult::Value(_) | RenderResult::Float(_) => {
            return Err("unexpected alert render result".into())
        }
    };
    Ok(serde_json::json!({"current":current,"rendered":rendered}))
}
