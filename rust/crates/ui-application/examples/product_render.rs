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
#[path = "support/product_effects.rs"]
mod product_effects;
#[path = "support/product_input.rs"]
mod product_input;
#[path = "support/product_network.rs"]
mod product_network;
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
    ssh_host: Option<String>,
    wifi: Option<openpilot_wifi::Snapshot>,
    #[serde(default)]
    capture_frames: Vec<u32>,
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
    let product_widgets::Product {
        widget,
        dialogs: dialog_results,
        network,
    } = product_widgets::create(&context, &mut canvas, &scene)?;
    if let Some(host) = &scene.ssh_host {
        product_input::ssh_fetcher(&widget, scene.config.big)?
            .borrow_mut()
            .host = host.clone();
    }
    widget.borrow_mut()?.set_rect(scene.rect);
    let keyboard = KeyboardInput::default();
    let navigation = NavigationQueue::default();
    let mut results = Vec::new();
    let mut effects = product_effects::Effects::new(context.clone());
    let mut last_event = openpilot_ui_framework::geometry::MouseEvent::default();
    widget.borrow_mut()?.show(&Frame {
        index: 0,
        now: 0.0,
        monotonic: 0.0,
        keyboard: &keyboard,
        navigation: &navigation,
        dt: 0.05,
        target_fps: 20.0,
        awake: true,
        events: &[],
        last_event,
        cursor: last_event.pos,
        wheel: 0.0,
        show_touches: false,
    });
    for index in 0..scene.frames {
        let now = f64::from(index) / 20.0;
        clock.set(now);
        let step = scene.steps.iter().find(|step| step.frame == index);
        product_input::apply(&context, step, now)?;
        product_input::scroll(&widget, step)?;
        if let Some(network) = &network {
            network.before(step);
        }
        effects.before(step)?;
        if step.is_some_and(|step| step.flush_ssh) {
            let fetcher = product_input::ssh_fetcher(&widget, scene.config.big)?;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while fetcher.borrow().is_fetching() {
                if std::time::Instant::now() >= deadline {
                    return Err("owned SSH fixture did not finish".into());
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
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
        if step.is_some_and(|step| step.show_again) {
            widget.borrow_mut()?.hide(&frame);
            widget.borrow_mut()?.show(&frame);
        }
        if let Some(network) = &network {
            network.ticks.run()?;
        }
        canvas.renderer.begin();
        widget.borrow_mut()?.render(&frame, &mut canvas)?;
        if let Some(request) = widget.borrow_mut()?.take_navigation() {
            if scene.dialog.is_some() {
                if let openpilot_ui_framework::widget::NavigationRequest::Pop(Some(callback)) =
                    request
                {
                    callback();
                }
            } else {
                effects.navigation(request)?;
            }
        }
        if scene.capture_effects {
            effects.drain(&navigation, &mut canvas)?;
            let mut snapshot = effects.snapshot(scene.raw_params.keys().cloned())?;
            if let Some(network) = &network {
                snapshot["network"] = network.snapshot(&widget)?;
            }
            results.push(snapshot);
        } else if scene.dialog.is_some() {
            let nav = widget.get::<openpilot_ui_framework::navigation::NavWidget>()?;
            let input = (nav.content.as_ref() as &dyn std::any::Any)
                .downcast_ref::<openpilot_ui_application::mici::widgets::dialog::InputDialog>(
            );
            results.push(serde_json::json!({"callbacks":*dialog_results.borrow(),"dismissing":nav.motion.is_dismissing(),"text":input.map(|input|input.text()),"candidate":input.map(|input|input.candidate())}));
        } else {
            results.push(serde_json::json!({"prime":context.prime.get()}));
        }
        if scene.capture_frames.contains(&index) {
            let stem = output
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or("invalid output stem")?;
            canvas
                .renderer
                .screenshot(&output.with_file_name(format!("{stem}-frame-{index:04}.png")))?;
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
