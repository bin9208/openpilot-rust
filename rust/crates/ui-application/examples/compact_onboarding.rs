use openpilot_startup_ui::{config::Config, renderer::Renderer};
use openpilot_ui_application::{
    context::Action,
    mici::layouts::{
        cards::Cards,
        onboarding::{Onboarding, QueuedCards, QueuedTutorial},
    },
    mici::onroad::driver_camera::Preview,
    params::Read,
};
use openpilot_ui_framework::{
    canvas::Canvas,
    geometry::{MouseEvent, Rect},
    keys::KeyboardInput,
    navigation::NavWidget,
    stack::NavigationStack,
    widget::{Frame, NavigationQueue, WidgetHandle},
    Error,
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
#[path = "support/tutorial_input.rs"]
mod tutorial_input;
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
}
#[derive(Deserialize)]
struct Step {
    frame: u32,
    #[serde(default)]
    events: Vec<MouseEvent>,
    page: Option<String>,
    driver: Option<tutorial_input::Driver>,
    awake: Option<bool>,
    timeout: Option<bool>,
    close: Option<bool>,
    lifecycle: Option<bool>,
    scroll_item: Option<usize>,
}
fn with_cards<T>(
    widget: &WidgetHandle,
    page: &str,
    f: impl FnOnce(&mut Cards) -> Result<T, Error>,
) -> Result<T, Error> {
    if page == "terms" {
        f(&mut widget.get_mut::<Onboarding>()?.terms.cards)
    } else {
        let mut widget = widget.get_mut::<NavWidget>()?;
        let cards = (widget.content.as_mut() as &mut dyn std::any::Any)
            .downcast_mut::<QueuedCards>()
            .ok_or(Error::Contract("cards type"))?;
        f(&mut cards.cards)
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, scene, output] = args.as_slice() else {
        return Err("compact_onboarding ROOT SCENE OUTPUT".into());
    };
    let scene: Scene = serde_json::from_slice(&std::fs::read(scene)?)?;
    let root = Path::new(root);
    let output = Path::new(output);
    let mut context = context::context(root, &scene, &output.with_extension("owned"))?;
    let clock = Rc::new(Cell::new(0.0));
    let time = clock.clone();
    context.now_monotonic = Rc::new(move || time.get());
    product_input::initialize(&context, (0, false))?;
    let assets = root.join("openpilot/selfdrive/assets");
    let mut canvas = Canvas::new(
        Renderer::new(scene.config, &assets, false, &scene.language)?,
        &assets,
    );
    let effects = Rc::new(RefCell::new(Vec::<String>::new()));
    let callback = |name: &'static str| {
        let effects = effects.clone();
        Rc::new(move || effects.borrow_mut().push(name.into())) as Rc<dyn Fn()>
    };
    let preview = Preview::with_camera(context.clone(), &mut canvas, "rustvision", 20.0, true)?;
    let onboarding =
        Onboarding::with_preview(context.clone(), &mut canvas, callback("completed"), preview)?;
    let training = onboarding.training.clone();
    let pre_dm = onboarding.pre_dm.clone();
    let tutorial = onboarding.tutorial.clone();
    let record = onboarding.record_front.clone();
    let widget = WidgetHandle::new(onboarding);
    let mut stack = NavigationStack::default();
    let navigation = NavigationQueue::default();
    let keyboard = KeyboardInput::default();
    let mut last_event = MouseEvent::default();
    let mut results = Vec::new();
    for index in 0..scene.frames {
        let now = f64::from(index) / 20.0;
        clock.set(now);
        let step = scene.steps.iter().find(|s| s.frame == index);
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
            stack.push(widget.clone(), &frame)?;
        }
        if let Some(show) = step.and_then(|s| s.lifecycle) {
            if show {
                widget.borrow_mut()?.show(&frame);
            } else {
                widget.borrow_mut()?.hide(&frame);
            }
        }
        if let Some(awake) = step.and_then(|s| s.awake) {
            context.device.borrow_mut().awake = awake;
        }
        if step.and_then(|s| s.timeout) == Some(true) {
            context.event(openpilot_ui_application::context::Event::InteractiveTimeout);
        }
        if step.and_then(|s| s.close) == Some(true) {
            widget.get::<Onboarding>()?.close()?;
        }
        if let Some(driver) = step.and_then(|s| s.driver.as_ref()) {
            tutorial_input::messages(&context, driver, index)?;
        }
        if let Some(index) = step.and_then(|s| s.scroll_item) {
            let page = step.and_then(|s| s.page.as_deref()).unwrap_or("terms");
            let target = match page {
                "terms" => &widget,
                "attention" => &training,
                "pre-dm" => &pre_dm,
                _ => &record,
            };
            if page == "record-front" {
                let mut nav = target.get_mut::<NavWidget>()?;
                let cards = (nav.content.as_mut() as &mut dyn std::any::Any)
                    .downcast_mut::<Cards>()
                    .ok_or("record type")?;
                tutorial_input::scroll(cards, index)?;
            } else {
                with_cards(target, page, |cards| tutorial_input::scroll(cards, index))?;
            }
        }
        tutorial_input::synchronize(index)?;
        canvas.renderer.begin();
        stack.render(
            &frame,
            Rect {
                x: 0.0,
                y: 0.0,
                width: 536.0,
                height: 240.0,
            },
            &mut canvas,
        )?;
        while let Some(action) = context.actions.pop() {
            match action {
                Action::MiciConfirm(options) => {
                    effects.borrow_mut().push(String::from("confirm"));
                    let dialog = openpilot_ui_application::mici::widgets::dialog::confirmation(
                        &mut canvas,
                        options,
                    )?;
                    stack.push(WidgetHandle::new(dialog), &frame)?;
                }
                Action::SetInteractiveTimeout(value) => {
                    context.device.borrow_mut().override_interactive_timeout = value
                }
                Action::SetOffroadBrightness(value) => {
                    context.device.borrow_mut().set_offroad_brightness(value)
                }
                Action::Failure(e) => return Err(e.into()),
                _ => return Err("unexpected cards action".into()),
            }
        }
        context.params.flush()?;
        let nav = tutorial.get::<NavWidget>()?;
        let guide = (nav.content.as_ref() as &dyn std::any::Any)
            .downcast_ref::<QueuedTutorial>()
            .ok_or("tutorial type")?;
        results.push(serde_json::json!({"effects":*effects.borrow(),"depth":stack.len(),"driver_view":context.params.boolean("IsDriverViewEnabled")?,"record_front":context.params.boolean("RecordFront")?,"accepted":context.params.string("HasAcceptedTerms")?,"trained":context.params.string("CompletedTrainingVersion")?,"uninstall":context.params.boolean("DoUninstall")?,"completed":widget.get::<Onboarding>()?.completed(),"progress":guide.tutorial.progress.value,"good":guide.tutorial.progress.good_enabled,"frame":guide.tutorial.preview.has_frame(),"rhd":guide.tutorial.preview.is_rhd(),"timeout":context.device.borrow().override_interactive_timeout,"brightness":context.device.borrow().offroad_brightness}));
        let path = output.with_file_name(format!(
            "{}-{index}.png",
            output.file_stem().ok_or("output stem")?.to_string_lossy()
        ));
        canvas.renderer.screenshot(&path)?;
        canvas.renderer.end();
    }
    drop(stack);
    drop(widget);
    drop(training);
    drop(pre_dm);
    drop(tutorial);
    drop(record);
    let remaining_callbacks = context.callbacks.borrow().len();
    if remaining_callbacks != 0 {
        return Err("tutorial callback retained after destruction".into());
    }
    std::fs::write(
        output.with_extension("lifecycle.json"),
        serde_json::to_vec(
            &serde_json::json!({"remaining_callbacks":remaining_callbacks,"navigation_empty":navigation.pop().is_none()}),
        )?,
    )?;
    std::fs::write(
        output.with_extension("json"),
        serde_json::to_vec_pretty(&results)?,
    )?;
    Ok(())
}
