use openpilot_startup_ui::{config::Config, renderer::Renderer};
use openpilot_ui_application::widgets::{
    carrot_web::CarrotWeb, prime::PrimeWidget, setup::SetupWidget,
};
use openpilot_ui_framework::{
    canvas::Canvas,
    geometry::Rect,
    keys::KeyboardInput,
    widget::{Frame, NavigationQueue, WidgetHandle},
};
use serde::Deserialize;
use std::{cell::Cell, collections::BTreeMap, path::Path, rc::Rc};
#[path = "support/context.rs"]
mod context;
#[path = "support/product_input.rs"]
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
    let widget = if scene.kind == "toggles" {
        if scene.config.big {
            openpilot_ui_application::settings::toggles::Toggles::create(
                context.clone(),
                &mut canvas,
            )?
        } else {
            openpilot_ui_application::mici::settings::toggles::Toggles::create(
                context.clone(),
                &mut canvas,
            )?
        }
    } else {
        WidgetHandle::from_box(match scene.kind.as_str() {
            "firehose" => {
                let widget =
                    openpilot_ui_application::widgets::firehose::Firehose::new(context.clone())?;
                if scene.config.big {
                    Box::new(widget)
                } else {
                    Box::new(widget.navigation(&canvas))
                }
            }
            "ssh" => Box::new(openpilot_ui_application::widgets::ssh::SshAction::new(
                context.clone(),
                &mut canvas,
            )?),
            "prime" => Box::new(PrimeWidget::new(context.clone())),
            "setup" => Box::new(SetupWidget::new(context.clone())),
            "pairing" if !scene.config.big => {
                let mut widget = openpilot_ui_application::mici::widgets::pairing::Pairing::new(
                    context.clone(),
                    &mut canvas,
                )?;
                widget.url = Box::new(|| "https://connect.comma.ai/?pair=fixture".into());
                Box::new(widget.navigation(context.clone(), &canvas))
            }
            "pairing" => {
                let mut widget = openpilot_ui_application::widgets::pairing::Pairing::new(
                    context.clone(),
                    &mut canvas,
                )?;
                widget.url = Box::new(|| "https://connect.comma.ai/?pair=fixture".into());
                Box::new(widget)
            }
            "carrot-web" => Box::new(CarrotWeb::new(context.clone())),
            _ => return Err("unknown product kind".into()),
        })
    };
    widget.borrow_mut()?.set_rect(scene.rect);
    let keyboard = KeyboardInput::default();
    let navigation = NavigationQueue::default();
    let mut results = Vec::new();
    let mut effects = Vec::new();
    let mut confirmations: Vec<openpilot_ui_application::context::Confirmation> = Vec::new();
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
        let events = step.map_or(&[][..], |step| step.events.as_slice());
        if let Some(event) = events.last() {
            last_event = *event;
        }
        let frame = Frame {
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
            results.push(serde_json::json!({"params":params,"effects":effects,"personality":context.ui.borrow().personality}));
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
