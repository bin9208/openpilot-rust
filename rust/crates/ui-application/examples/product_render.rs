use openpilot_startup_ui::renderer::Renderer;
use openpilot_ui_framework::{
    canvas::Canvas,
    keys::KeyboardInput,
    widget::{Frame, NavigationQueue},
};
use std::{cell::Cell, path::Path, rc::Rc};
#[path = "support/context.rs"]
mod context;
#[path = "support/product_alert.rs"]
mod product_alert;
#[path = "support/product_camera.rs"]
mod product_camera;
#[path = "support/product_driver.rs"]
mod product_driver;
#[path = "support/product_effects.rs"]
mod product_effects;
#[path = "support/product_egpu.rs"]
mod product_egpu;
#[path = "support/product_indicator.rs"]
mod product_indicator;
#[path = "support/product_input.rs"]
mod product_input;
#[path = "support/product_network.rs"]
mod product_network;
#[path = "support/product_scene.rs"]
mod product_scene;
#[path = "support/product_settings.rs"]
mod product_settings;
#[path = "support/product_widgets.rs"]
mod product_widgets;
pub use product_scene::{DialogProbe, Scene};
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
        egpu,
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
        if scene.camera.is_some() {
            product_camera::before(&context, &widget, &scene, index)?;
        }
        let now = f64::from(index) / 20.0;
        clock.set(
            scene
                .alert
                .as_ref()
                .map_or(now, |options| options.now(index)),
        );
        let step = scene.steps.iter().find(|step| step.frame == index);
        product_input::apply(&context, step, now)?;
        if let Some(alert) = &scene.alert {
            product_alert::before(&context, alert, index)?;
        }
        if let Some(indicator) = &scene.indicator {
            product_indicator::before(&context, &widget, indicator, index)?;
        }
        product_input::scroll(&widget, step)?;
        if let Some(network) = &network {
            network.before(step);
        }
        if let Some(egpu) = &egpu {
            egpu.before(index);
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
        if scene
            .driver
            .as_ref()
            .is_some_and(|driver| !driver.navigation)
            && !scene.config.big
            && index == 20
        {
            widget.borrow_mut()?.hide(&frame);
        }
        if scene
            .driver
            .as_ref()
            .is_some_and(|driver| !driver.navigation)
            && !scene.config.big
            && index == 21
        {
            widget.borrow_mut()?.show(&frame);
        }
        if step.is_some_and(|step| step.show_again) {
            widget.borrow_mut()?.hide(&frame);
            widget.borrow_mut()?.show(&frame);
        }
        if let Some(network) = &network {
            network.ticks.run()?;
        }
        canvas.renderer.begin();
        let rendered = widget.borrow_mut()?.render(&frame, &mut canvas)?;
        if scene
            .driver
            .as_ref()
            .is_some_and(|driver| scene.config.big || driver.navigation)
            && index + 1 == scene.frames
        {
            widget.borrow_mut()?.hide(&frame);
        }
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
            if scene.kind == "settings-root" {
                snapshot["settings"] = if scene.config.big {
                    serde_json::json!(format!(
                        "{:?}",
                        widget
                            .get::<openpilot_ui_application::settings::layout::Settings>()?
                            .current()
                    ))
                } else {
                    serde_json::Value::Null
                };
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
        if scene.camera.is_some() {
            results.last_mut().ok_or("missing trace")?["camera"] =
                product_camera::snapshot(&context, &widget, &scene)?;
        }
        if scene.alert.is_some() {
            results.last_mut().ok_or("missing alert trace")?["alert"] =
                product_alert::snapshot(&widget, scene.config.big, rendered)?;
        }
        if scene.indicator.is_some() {
            results.last_mut().ok_or("missing indicator trace")?["indicator"] =
                product_indicator::snapshot(&widget, &scene.kind)?;
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
    if let Some(egpu) = &egpu {
        std::fs::write(
            output.with_extension("egpu.json"),
            serde_json::to_vec_pretty(&egpu.snapshot())?,
        )?;
    }
    if scene.driver.is_some() {
        drop(widget);
        std::fs::write(
            output.with_extension("driver-lifecycle.json"),
            serde_json::to_vec_pretty(
                &serde_json::json!({"remaining_callbacks":context.callbacks.borrow().len()}),
            )?,
        )?;
    }
    Ok(())
}
