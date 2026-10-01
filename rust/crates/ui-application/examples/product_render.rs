use openpilot_startup_ui::{config::Config, renderer::Renderer};
use openpilot_ui_framework::{
    canvas::Canvas,
    geometry::Rect,
    keys::KeyboardInput,
    widget::{Frame, NavigationQueue},
};
use serde::Deserialize;
use std::{cell::Cell, collections::BTreeMap, path::Path, rc::Rc};
#[path = "support/context.rs"]
mod context;
#[path = "support/product_input.rs"]
mod product_input;
#[path = "support/product_widgets.rs"]
mod product_widgets;
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
    #[serde(default)]
    address: Option<String>,
    #[serde(default)]
    network_type: u16,
    #[serde(default)]
    network_metered: bool,
    #[serde(default)]
    car: Option<openpilot_ui_application::state::CarConfig>,
    #[serde(default)]
    models: openpilot_ui_application::state::ModelStatus,
    #[serde(default)]
    steps: Vec<product_input::Step>,
    #[serde(default)]
    capture_effects: bool,
    #[serde(default)]
    dialog: Option<DialogProbe>,
    time_valid: Option<bool>,
}
#[derive(Deserialize)]
pub struct DialogProbe {
    title: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    red: bool,
    #[serde(default)]
    stay: bool,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, scene, output] = args.as_slice() else {
        return Err("product_render ROOT SCENE OUTPUT".into());
    };
    let scene: Scene = serde_json::from_slice(&std::fs::read(scene)?)?;
    let root = Path::new(root);
    let output = Path::new(output);
    let mut context = context::context(root, &scene, &output.with_extension("owned"))?;
    let clock = Rc::new(Cell::new(0.0));
    let now = clock.clone();
    context.now_monotonic = Rc::new(move || now.get());
    product_input::initialize(&context, (scene.network_type, scene.network_metered))?;
    let assets = root.join("openpilot/selfdrive/assets");
    let renderer = Renderer::new(scene.config, &assets, false, &scene.language)?;
    let mut canvas = Canvas::new(renderer, &assets);
    let (widget, dialog_results) = product_widgets::create(&context, &mut canvas, &scene)?;
    widget.borrow_mut()?.set_rect(scene.rect);
    let keyboard = KeyboardInput::default();
    let navigation = NavigationQueue::default();
    let mut results = Vec::new();
    let mut effects = Vec::new();
    let mut confirmations: Vec<openpilot_ui_application::context::Confirmation> = Vec::new();
    let mut mici_confirmations =
        Vec::<openpilot_ui_application::mici::widgets::dialog::Confirmation>::new();
    let mut last_event = openpilot_ui_framework::geometry::MouseEvent::default();
    for index in 0..scene.frames {
        let now = f64::from(index) / 20.0;
        clock.set(now);
        let step = scene.steps.iter().find(|step| step.frame == index);
        product_input::apply(&context, step, now)?;
        product_input::scroll(&widget, step)?;
        if let Some(confirm) = step.and_then(|step| step.confirm) {
            if let Some(dialog) = confirmations.pop() {
                dialog.callback.call(if confirm {
                    openpilot_ui_framework::widget::DialogResult::Confirm
                } else {
                    openpilot_ui_framework::widget::DialogResult::Cancel
                });
            }
        }
        if let Some(result) = step.and_then(|step| step.confirm) {
            if let Some(dialog) = mici_confirmations.pop() {
                if result {
                    (dialog.callback)();
                }
            }
        }
        let events = step.map_or(&[][..], |step| step.events.as_slice());
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
            wheel: step.map_or(0.0, |step| step.wheel),
            show_touches: false,
        };
        if index == 0 {
            widget.borrow_mut()?.show(&frame);
        }
        canvas.renderer.begin();
        widget.borrow_mut()?.render(&frame, &mut canvas)?;
        if scene.dialog.is_some() || scene.kind == "regulatory" {
            if let Some(openpilot_ui_framework::widget::NavigationRequest::Pop(callback)) =
                widget.borrow_mut()?.take_navigation()
            {
                if scene.kind == "regulatory" {
                    effects.push(serde_json::json!({"pop":true}));
                }
                if let Some(callback) = callback {
                    callback();
                }
            }
        }
        while let Some(request) = navigation.pop() {
            if let openpilot_ui_framework::widget::NavigationRequest::Pop(callback) = request {
                effects.push(serde_json::json!({"pop":true}));
                if let Some(callback) = callback {
                    callback();
                }
            } else {
                return Err("unexpected native widget navigation".into());
            }
        }
        if scene.capture_effects {
            while let Some(action) = context.actions.pop() {
                use openpilot_ui_application::context::Action;
                match action {
                    Action::Confirm(dialog) => {
                        effects.push(serde_json::json!({"confirm":dialog.text,"button":dialog.confirm,"cancel":dialog.cancel,"rich":dialog.rich}));
                        confirmations.push(dialog);
                    }
                    Action::ShowTouches(value) => {
                        effects.push(serde_json::json!({"touches":value}))
                    }
                    Action::ShowFps(value) => effects.push(serde_json::json!({"fps":value})),
                    Action::MiciAlert { title, description } => effects
                        .push(serde_json::json!({"mici_alert":title,"description":description})),
                    Action::MiciConfirm(dialog) => {
                        effects.push(serde_json::json!({"mici_confirm":dialog.title,"exit":dialog.exit_on_confirm,"red":dialog.red}));
                        mici_confirmations.push(dialog);
                    }
                    Action::Updater(action) => {
                        if matches!(
                            action,
                            openpilot_ui_application::context::actions::UpdaterAction::Reboot
                        ) {
                            context.params.put_bool("DoReboot", true)?;
                        } else {
                            effects.push(serde_json::json!({"updater":format!("{action:?}")}));
                        }
                    }
                    Action::SetLanguage(code) => {
                        canvas.renderer.set_language(&code);
                        effects.push(serde_json::json!({"language":code}));
                    }
                    Action::Alert(value) => effects.push(serde_json::json!({"alert":value})),
                    Action::Open(page) => {
                        effects.push(serde_json::json!({"page":format!("{page:?}")}))
                    }
                    Action::Failure(error) => return Err(error.into()),
                    _ => return Err("unexpected product effect".into()),
                }
            }
            context.params.flush()?;
            use openpilot_ui_application::params::Read;
            let params = product_input::KEYS
                .iter()
                .map(|key| {
                    Ok((
                        key.to_string(),
                        context
                            .params
                            .bytes(key)?
                            .map(String::from_utf8)
                            .transpose()?,
                    ))
                })
                .collect::<Result<BTreeMap<_, _>, Box<dyn std::error::Error>>>()?;
            let raw_params = scene
                .raw_params
                .keys()
                .filter_map(|key| match context.params.bytes(key) {
                    Ok(Some(bytes)) => Some(Ok((
                        key.clone(),
                        bytes
                            .iter()
                            .map(|byte| format!("{byte:02x}"))
                            .collect::<String>(),
                    ))),
                    Ok(None) => None,
                    Err(error) => Some(Err(error)),
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?;
            results.push(serde_json::json!({"params":params,"effects":effects,"personality":context.ui.borrow().personality,"raw_params":raw_params}));
        } else if scene.dialog.is_some() {
            let nav = widget.get::<openpilot_ui_framework::navigation::NavWidget>()?;
            let input = (nav.content.as_ref() as &dyn std::any::Any)
                .downcast_ref::<openpilot_ui_application::mici::widgets::dialog::InputDialog>(
            );
            results.push(serde_json::json!({"callbacks":*dialog_results.borrow(),"dismissing":nav.motion.is_dismissing(),"text":input.map(|input|input.text()),"candidate":input.map(|input|input.candidate())}));
        } else {
            results.push(serde_json::json!({"prime":context.prime.get()}));
        }
        if index + 1 == scene.frames {
            canvas.renderer.screenshot(output)?;
        }
        canvas.renderer.end();
    }
    std::fs::write(
        output.with_extension("json"),
        serde_json::to_vec_pretty(&results)?,
    )?;
    Ok(())
}
