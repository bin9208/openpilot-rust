use openpilot_ui_application::{
    context::{Context, Event},
    state::CarConfig,
};
use openpilot_ui_framework::geometry::MouseEvent;
use serde::Deserialize;
use std::collections::BTreeMap;
#[derive(Default, Deserialize)]
pub struct Step {
    pub frame: u32,
    pub wifi: Option<openpilot_wifi::Snapshot>,
    #[serde(default)]
    pub wifi_events: Vec<openpilot_wifi::Event>,
    pub prime: Option<i32>,
    #[serde(default)]
    pub show_again: bool,
    #[serde(default)]
    pub events: Vec<MouseEvent>,
    #[serde(default)]
    pub wheel: f64,
    pub scroll: Option<f64>,
    pub confirm: Option<bool>,
    pub input_text: Option<String>,
    pub selection: Option<String>,
    #[serde(default)]
    pub flush_ssh: bool,
    pub engaged: Option<bool>,
    pub started: Option<bool>,
    pub ignition: Option<bool>,
    pub personality: Option<u16>,
    #[serde(default)]
    pub params: BTreeMap<String, Option<String>>,
}
pub fn car_bytes(car: CarConfig) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut message = capnp::message::Builder::new_default();
    let mut cp = message.init_root::<openpilot_cereal::car_capnp::car_params::Builder>();
    cp.set_alpha_longitudinal_available(car.alpha_longitudinal_available);
    cp.set_openpilot_longitudinal_control(car.openpilot_longitudinal_control);
    cp.set_max_lateral_accel(openpilot_ui_framework::text_layout::float(
        car.max_lateral_accel,
    ));
    Ok(capnp::serialize::write_message_to_words(&message))
}
pub fn initialize(
    context: &Context,
    network: (u16, bool),
) -> Result<(), Box<dyn std::error::Error>> {
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<openpilot_cereal::log_capnp::event::Builder>();
    event.set_valid(true);
    let mut device = event.init_device_state();
    device.set_network_type(network.0.try_into()?);
    device.set_network_metered(network.1);
    context
        .messages
        .borrow_mut()
        .state
        .update(0.0, &[capnp::serialize::write_message_to_words(&message)])?;
    Ok(())
}
pub fn apply(
    context: &Context,
    step: Option<&Step>,
    now: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut messages = Vec::new();
    if let Some(step) = step {
        if let Some(prime) = step.prime {
            context.prime.set(prime);
        }
        for (key, value) in &step.params {
            if let Some(value) = value {
                context.params.put(key, value.as_bytes())?;
            } else {
                context.params.remove(key)?;
            }
        }
        if let Some(ignition) = step.ignition {
            context.ui.borrow_mut().ignition = ignition;
        }
        if let Some(started) = step.started {
            context.ui.borrow_mut().started = started;
            context.event(Event::Offroad);
        }
        if let Some(engaged) = step.engaged {
            context.ui.borrow_mut().engaged = engaged;
            context.event(Event::Engaged);
        }
        if let Some(personality) = step.personality {
            let mut message = capnp::message::Builder::new_default();
            let mut event = message.init_root::<openpilot_cereal::log_capnp::event::Builder>();
            event.set_valid(true);
            event
                .init_selfdrive_state()
                .set_personality(personality.try_into()?);
            messages.push(capnp::serialize::write_message_to_words(&message));
        }
    }
    context.messages.borrow_mut().state.update(now, &messages)?;
    context.sync_services();
    Ok(())
}
pub const KEYS: &[&str] = &[
    "ExperimentalMode",
    "ExperimentalModeConfirmed",
    "OnroadCycleRequested",
    "LongitudinalPersonality",
    "IsMetric",
    "RecordAudio",
    "RecordFront",
    "OpenpilotEnabledToggle",
    "AlphaLongitudinalEnabled",
    "DevicePosition",
    "DoReboot",
    "DoShutdown",
    "DoUninstall",
    "LanguageSetting",
    "AdbEnabled",
    "SshEnabled",
    "JoystickDebugMode",
    "LongitudinalManeuverMode",
    "ShowDebugInfo",
    "GithubUsername",
    "GithubSshKeys",
    "UpdaterTargetBranch",
    "GsmRoaming",
    "GsmMetered",
    "GsmApn",
];

pub fn scroll(
    widget: &openpilot_ui_framework::widget::WidgetHandle,
    step: Option<&Step>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(position) = step.and_then(|step| step.scroll) {
        let mut nav = widget.get_mut::<openpilot_ui_framework::navigation::NavWidget>()?;
        let content = nav.content.as_mut() as &mut dyn std::any::Any;
        if let Some(content) =
            content.downcast_mut::<openpilot_ui_application::mici::settings::toggles::Toggles>()
        {
            content.scroller.scroll_to(position, false, false, false)?;
        } else if let Some(content) =
            content.downcast_mut::<openpilot_ui_application::mici::settings::device::Device>()
        {
            content.scroller.scroll_to(position, false, false, false)?;
        } else if let Some(content) =
            content.downcast_mut::<openpilot_ui_application::mici::settings::developer::Developer>()
        {
            content.scroller.scroll_to(position, false, false, false)?;
        } else if let Some(content) =
            content.downcast_mut::<openpilot_ui_application::mici::settings::network::Network>()
        {
            content.scroller.scroll_to(position, false, false, false)?;
        } else if let Some(content) =
            content.downcast_mut::<openpilot_ui_application::mici::settings::network::wifi::Wifi>()
        {
            content.scroller.scroll_to(position, false, false, false)?;
        } else {
            return Err("unsupported scroll target".into());
        }
    }
    Ok(())
}

pub fn ssh_fetcher(
    widget: &openpilot_ui_framework::widget::WidgetHandle,
    big: bool,
) -> Result<
    std::rc::Rc<std::cell::RefCell<openpilot_ui_application::services::ssh::Fetcher>>,
    Box<dyn std::error::Error>,
> {
    if big {
        let mut panel =
            widget.get_mut::<openpilot_ui_application::settings::developer::Developer>()?;
        let item = panel
            .scroller
            .item_mut::<openpilot_ui_framework::list::ListItem>(2)
            .ok_or("SSH item missing")?;
        Ok(item
            .action_mut::<openpilot_ui_application::widgets::ssh::SshAction>()
            .ok_or("SSH action missing")?
            .fetcher
            .clone())
    } else {
        let mut nav = widget.get_mut::<openpilot_ui_framework::navigation::NavWidget>()?;
        let panel = (nav.content.as_mut() as &mut dyn std::any::Any)
            .downcast_mut::<openpilot_ui_application::mici::settings::developer::Developer>()
            .ok_or("Mici developer panel missing")?;
        Ok(panel.fetcher.clone())
    }
}
