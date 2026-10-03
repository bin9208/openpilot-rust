use openpilot_startup_ui::{config::Config, renderer::Renderer};
use openpilot_ui_application::{
    context::{actions::UpdaterAction, Action},
    mici::layouts::offroad_alerts::{AlertItem, AlertSize, OffroadAlerts},
};
use openpilot_ui_framework::{
    canvas::Canvas,
    geometry::{MouseEvent, Rect},
    keys::KeyboardInput,
    widget::{Frame, NavigationQueue, Widget},
};
use serde::Deserialize;
use std::{cell::Cell, collections::BTreeMap, path::Path, rc::Rc};
#[path = "support/context.rs"]
mod context;
#[path = "support/home_input.rs"]
mod product_input;
#[derive(Deserialize)]
pub struct Scene {
    config: Config,
    language: String,
    frames: u32,
    prime: i32,
    #[serde(default)]
    params: BTreeMap<String, String>,
    #[serde(default)]
    raw_params: BTreeMap<String, Vec<u8>>,
    address: Option<String>,
    car: Option<openpilot_ui_application::state::CarConfig>,
    #[serde(default)]
    models: openpilot_ui_application::state::ModelStatus,
    time_valid: Option<bool>,
    #[serde(default)]
    steps: Vec<Step>,
    capture_frames: Vec<u32>,
    rect: Rect,
}
#[derive(Deserialize)]
struct Step {
    frame: u32,
    now: Option<f64>,
    #[serde(default)]
    events: Vec<MouseEvent>,
    #[serde(default)]
    params: BTreeMap<String, String>,
    #[serde(default)]
    remove: Vec<String>,
    #[serde(default)]
    refresh: bool,
    #[serde(default)]
    show: bool,
    offset: Option<f64>,
}
fn snapshot(
    alerts: &OffroadAlerts,
    effects: &[String],
) -> Result<serde_json::Value, openpilot_ui_framework::Error> {
    let mut items = Vec::new();
    for index in 0..alerts.scroller.len() {
        let widget = alerts
            .scroller
            .item(index)
            .ok_or(openpilot_ui_framework::Error::Contract(
                "alert item missing",
            ))?;
        let item = (widget as &dyn std::any::Any)
            .downcast_ref::<AlertItem>()
            .ok_or(openpilot_ui_framework::Error::Contract("alert item type"))?;
        let size = match item.alert_size {
            AlertSize::Small => 0,
            AlertSize::Medium => 1,
            AlertSize::Big => 2,
        };
        items.push(serde_json::json!({"data":item.alert_data,"rect":item.state.rect,"size":size,"title":item.split().0,"body":item.split().1,"pressed":item.state.is_pressed(),"visible":item.state.visible.get()}));
    }
    Ok(
        serde_json::json!({"active":alerts.active_alerts(),"scrolling":alerts.scrolling(),"offset":alerts.scroller.scroll_offset,"content":alerts.scroller.content_size,"effects":effects,"items":items}),
    )
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, scene, output] = args.as_slice() else {
        return Err("mici_offroad_alerts ROOT SCENE OUTPUT".into());
    };
    let scene: Scene = serde_json::from_slice(&std::fs::read(scene)?)?;
    let root = Path::new(root);
    let output = Path::new(output);
    let mut context = context::context(root, &scene, &output.with_extension("owned"))?;
    product_input::initialize(&context, (0, false))?;
    let clock = Rc::new(Cell::new(0.0));
    let time = clock.clone();
    context.now_monotonic = Rc::new(move || time.get());
    let assets = root.join("openpilot/selfdrive/assets");
    let mut canvas = Canvas::new(
        Renderer::new(scene.config, &assets, false, &scene.language)?,
        &assets,
    );
    let mut alerts = OffroadAlerts::new(context.clone(), &mut canvas)?;
    let keyboard = KeyboardInput::default();
    let navigation = NavigationQueue::default();
    let mut last_event = MouseEvent::default();
    let mut results = Vec::new();
    let mut effects = Vec::new();
    let mut now = 0.0;
    for index in 0..scene.frames {
        let step = scene.steps.iter().find(|step| step.frame == index);
        now = step
            .and_then(|step| step.now)
            .unwrap_or(if index == 0 { 0.0 } else { now + 0.05 });
        clock.set(now);
        let events = step.map_or(&[][..], |step| step.events.as_slice());
        if let Some(event) = events.last() {
            last_event = *event
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
        if let Some(step) = step {
            for (key, value) in &step.params {
                context.params.put(key, value.as_bytes())?
            }
            for key in &step.remove {
                context.params.remove(key)?
            }
            if let Some(offset) = step.offset {
                alerts.scroller.panel.set_offset(offset)
            }
            if step.refresh {
                alerts.refresh()?;
            }
        }
        if index == 0 || step.is_some_and(|step| step.show) {
            alerts.show(&frame)
        }
        alerts.set_rect(scene.rect);
        canvas.renderer.begin();
        alerts.render(&frame, &mut canvas)?;
        while let Some(action) = context.actions.pop() {
            match action {
                Action::Updater(UpdaterAction::Reboot) => effects.push("reboot".into()),
                Action::Failure(error) => return Err(error.into()),
                _ => return Err("unexpected offroad alerts action".into()),
            }
        }
        results.push(snapshot(&alerts, &effects)?);
        if scene.capture_frames.contains(&index) {
            canvas.renderer.screenshot(&output.with_file_name(format!(
                "{}-{index}.png",
                output.file_stem().ok_or("output stem")?.to_string_lossy()
            )))?;
        }
        canvas.renderer.end();
    }
    std::fs::write(
        output.with_extension("json"),
        serde_json::to_vec_pretty(&results)?,
    )?;
    Ok(())
}
