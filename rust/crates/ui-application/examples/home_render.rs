use openpilot_startup_ui::{config::Config, renderer::Renderer};
use openpilot_ui_application::layouts::{
    experimental::ExperimentalModeButton, home::Home, sidebar::Sidebar,
};
use openpilot_ui_framework::{
    callback::Callback,
    canvas::Canvas,
    geometry::{MouseEvent, Rect},
    keys::KeyboardInput,
    widget::{Frame, NavigationQueue, WidgetHandle},
};
use serde::Deserialize;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    path::Path,
    rc::Rc,
};
#[path = "support/context.rs"]
mod context;
#[path = "support/home_input.rs"]
mod product_input;
#[derive(Deserialize)]
pub struct Scene {
    kind: String,
    config: Config,
    language: String,
    rect: Rect,
    frames: u32,
    prime: i32,
    #[serde(default)]
    params: BTreeMap<String, String>,
    #[serde(default)]
    raw_params: BTreeMap<String, Vec<u8>>,
    address: Option<String>,
    #[serde(default)]
    network_type: u16,
    #[serde(default)]
    network_metered: bool,
    car: Option<openpilot_ui_application::state::CarConfig>,
    #[serde(default)]
    models: openpilot_ui_application::state::ModelStatus,
    time_valid: Option<bool>,
    #[serde(default)]
    steps: Vec<Step>,
}
#[derive(Deserialize)]
struct Step {
    frame: u32,
    now: Option<f64>,
    #[serde(default)]
    events: Vec<MouseEvent>,
    #[serde(default)]
    params: BTreeMap<String, Option<String>>,
    device: Option<Device>,
    recording: Option<bool>,
    panda: Option<u16>,
    scroll: Option<f64>,
    #[serde(default)]
    flush_training: bool,
}
#[derive(Deserialize)]
struct Device {
    network: u16,
    strength: u16,
    thermal: u16,
    ping: u64,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, scene, output] = args.as_slice() else {
        return Err("home_render ROOT SCENE OUTPUT".into());
    };
    let scene: Scene = serde_json::from_slice(&std::fs::read(scene)?)?;
    let root = Path::new(root);
    let output = Path::new(output);
    let mut context = context::context(root, &scene, &output.with_extension("owned"))?;
    let clock = Rc::new(Cell::new(0.0));
    let time = clock.clone();
    context.now_monotonic = Rc::new(move || time.get());
    product_input::initialize(&context, (scene.network_type, scene.network_metered))?;
    let assets = root.join("openpilot/selfdrive/assets");
    let renderer = Renderer::new(scene.config, &assets, false, &scene.language)?;
    let mut canvas = Canvas::new(renderer, &assets);
    let effects = Rc::new(RefCell::new(Vec::<String>::new()));
    let callback = |name: &'static str| {
        let effects = effects.clone();
        Callback::new(move |()| effects.borrow_mut().push(name.into()))
    };
    let widget = match scene.kind.as_str() {
        "onboarding" => WidgetHandle::new(
            openpilot_ui_application::layouts::onboarding::Onboarding::new(context.clone())?,
        ),
        "sidebar" => {
            let mut w = Sidebar::new(context.clone(), &mut canvas)?;
            let clock = clock.clone();
            w.monotonic_ns = Rc::new(move || {
                num_traits::ToPrimitive::to_u128(&(clock.get() * 1e9)).ok_or(
                    openpilot_ui_framework::Error::Contract("fixture monotonic nanoseconds"),
                )
            });
            w.on_settings = Some(callback("settings"));
            w.on_carrot_web = Some(callback("web"));
            w.open_settings = Some(callback("microphone"));
            WidgetHandle::new(w)
        }
        "mici-home" => {
            let mut w = openpilot_ui_application::mici::layouts::home::Home::new(
                context.clone(),
                &mut canvas,
            )?;
            w.on_settings = Some(callback("settings"));
            w.on_carrot_web = Some(callback("web"));
            WidgetHandle::new(w)
        }
        "home" => {
            let mut w = Home::new(context.clone(), &mut canvas)?;
            w.set_settings_callback(callback("settings"));
            WidgetHandle::new(w)
        }
        "experimental" => {
            let mut w = ExperimentalModeButton::new(context.clone(), &mut canvas)?;
            let callback = callback("settings");
            w.state.click = Some(Box::new(move || callback.call(())));
            WidgetHandle::new(w)
        }
        _ => return Err("unknown layout".into()),
    };
    widget.borrow_mut()?.set_rect(scene.rect);
    let keyboard = KeyboardInput::default();
    let navigation = NavigationQueue::default();
    let mut last_event = MouseEvent::default();
    let mut results = Vec::new();
    for index in 0..scene.frames {
        let step = scene.steps.iter().find(|s| s.frame == index);
        let now = step.and_then(|s| s.now).unwrap_or(f64::from(index) / 20.0);
        clock.set(now);
        let mut updates = Vec::new();
        if let Some(step) = step {
            for (key, value) in &step.params {
                if let Some(value) = value {
                    context.params.put(key, value.as_bytes())?;
                } else {
                    context.params.remove(key)?;
                }
            }
            if let Some(value) = step.recording {
                context.ui.borrow_mut().recording_audio = value;
            }
            if let Some(value) = step.panda {
                context.ui.borrow_mut().panda_type = value;
            }
            if let Some(value) = &step.device {
                let mut m = capnp::message::Builder::new_default();
                let mut event = m.init_root::<openpilot_cereal::log_capnp::event::Builder>();
                event.set_valid(true);
                let mut ds = event.init_device_state();
                ds.set_last_athena_ping_time(value.ping);
                let capnp::dynamic_value::Builder::Struct(mut dynamic) = ds.into() else {
                    return Err("device dynamic schema".into());
                };
                for (name, value) in [
                    ("networkType", value.network),
                    ("networkStrength", value.strength),
                    ("thermalStatus", value.thermal),
                ] {
                    let capnp::introspect::TypeVariant::Enum(schema) = dynamic
                        .get_schema()
                        .get_field_by_name(name)?
                        .get_type()
                        .which()
                    else {
                        return Err("device enum schema".into());
                    };
                    dynamic.set_named(
                        name,
                        capnp::dynamic_value::Enum::new(
                            value,
                            capnp::schema::EnumSchema::new(schema),
                        )
                        .into(),
                    )?;
                }
                updates.push(capnp::serialize::write_message_to_words(&m));
            }
            if let Some(offset) = step.scroll {
                let mut home = widget.get_mut::<Home>()?;
                home.offroad_alert.scroll.set_offset(offset);
                home.update_alert.scroll.set_offset(offset);
            }
        }
        if step.is_some_and(|s| s.flush_training) {
            let mut onboarding =
                widget.get_mut::<openpilot_ui_application::layouts::onboarding::Onboarding>()?;
            if let Some(training) = &mut onboarding.training {
                training.finish_decode()?;
            }
        }
        context.messages.borrow_mut().state.update(now, &updates)?;
        let events = step.map_or(&[][..], |s| s.events.as_slice());
        if let Some(event) = events.last() {
            last_event = *event;
        }
        let frame = Frame {
            index: u64::from(index),
            now,
            monotonic: now,
            keyboard: &keyboard,
            navigation: &navigation,
            dt: 0.05,
            target_fps: 20.0,
            awake: true,
            events,
            last_event,
            cursor: last_event.pos,
            wheel: 0.0,
            show_touches: false,
        };
        if index == 0 {
            widget.borrow_mut()?.show(&frame);
        }
        canvas.renderer.begin();
        widget.borrow_mut()?.render(&frame, &mut canvas)?;
        while let Some(action) = context.actions.pop() {
            use openpilot_ui_application::context::Action;
            match action {
                Action::Updater(a) => effects.borrow_mut().push(format!("updater:{a:?}")),
                Action::Failure(e) => return Err(e.into()),
                Action::Exit => effects.borrow_mut().push("exit".into()),
                _ => return Err("unexpected layout action".into()),
            }
        }
        while let Some(request) = navigation.pop() {
            match request {
                openpilot_ui_framework::widget::NavigationRequest::Pop(callback) => {
                    effects.borrow_mut().push("pop".into());
                    if let Some(callback) = callback {
                        callback();
                    }
                }
                _ => return Err("unexpected navigation".into()),
            }
        }
        let state = match scene.kind.as_str() {
            "onboarding" => {
                let w =
                    widget.get::<openpilot_ui_application::layouts::onboarding::Onboarding>()?;
                serde_json::json!({"page":w.page.get(),"completed":w.completed(),"step":w.training.as_ref().map(|t|t.step),"uploaded":w.training.as_ref().map(|t|t.uploaded())})
            }
            "sidebar" => serde_json::to_value(&widget.get::<Sidebar>()?.status)?,
            "home" => {
                let h = widget.get::<Home>()?;
                serde_json::json!({"current":h.current,"update":h.update_available,"alerts":h.alert_count,"version":h.version,"last_refresh":h.last_refresh})
            }
            "mici-home" => {
                let h = widget.get::<openpilot_ui_application::mici::layouts::home::Home>()?;
                serde_json::json!({"version":h.version,"experimental":h.experimental,"address":h.address,"last_refresh":h.last_refresh,"did_long_press":h.did_long_press})
            }
            _ => {
                serde_json::json!({"experimental":widget.get::<ExperimentalModeButton>()?.experimental})
            }
        };
        use openpilot_ui_application::params::Read;
        results.push(serde_json::json!({"state":state,"effects":*effects.borrow(),"snooze":context.params.boolean("SnoozeUpdate")?,"excessive":context.params.string("Offroad_ExcessiveActuation")?,"accepted":context.params.string("HasAcceptedTerms")?,"trained":context.params.string("CompletedTrainingVersion")?,"record_front":context.params.boolean("RecordFront")?,"uninstall":context.params.boolean("DoUninstall")?}));
        let path = output.with_file_name(format!(
            "{}-{index}.png",
            output.file_stem().ok_or("output stem")?.to_string_lossy()
        ));
        canvas.renderer.screenshot(&path)?;
        canvas.renderer.end();
    }
    std::fs::write(
        output.with_extension("json"),
        serde_json::to_vec_pretty(&results)?,
    )?;
    Ok(())
}
