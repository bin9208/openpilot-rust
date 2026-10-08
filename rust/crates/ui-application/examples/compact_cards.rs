use openpilot_startup_ui::{config::Config, renderer::Renderer};
use openpilot_ui_application::{
    context::Action,
    mici::layouts::cards::{self, Cards},
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
#[derive(Deserialize)]
pub struct Scene {
    kind: String,
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
    scroll: Option<f64>,
    scroll_item: Option<usize>,
}
fn with_cards<T>(
    widget: &WidgetHandle,
    nav: bool,
    f: impl FnOnce(&mut Cards) -> Result<T, Error>,
) -> Result<T, Error> {
    if nav {
        let mut widget = widget.get_mut::<NavWidget>()?;
        let cards = (widget.content.as_mut() as &mut dyn std::any::Any)
            .downcast_mut::<Cards>()
            .ok_or(Error::Contract("cards type"))?;
        f(cards)
    } else {
        f(&mut *widget.get_mut::<Cards>()?)
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, scene, output] = args.as_slice() else {
        return Err("compact_cards ROOT SCENE OUTPUT".into());
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
    let cards = match scene.kind.as_str() {
        "terms" => cards::terms(
            context.clone(),
            &mut canvas,
            callback("accept"),
            callback("decline"),
        )?,
        "attention" => cards::attention(context.clone(), &mut canvas, callback("next"))?,
        "pre-dm" => cards::pre_dm(context.clone(), &mut canvas, callback("next"))?,
        "bad-face" => cards::bad_face(context.clone(), &mut canvas, callback("back"))?,
        "record-front" => cards::record_front(context.clone(), &mut canvas, callback("next"))?,
        _ => return Err("unknown cards".into()),
    };
    let nav = !matches!(scene.kind.as_str(), "terms" | "attention");
    let widget = if nav {
        WidgetHandle::new(cards.navigation())
    } else {
        WidgetHandle::new(cards)
    };
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
        if let Some(offset) = step.and_then(|s| s.scroll) {
            with_cards(&widget, nav, |c| {
                c.scroller.scroll_to(offset, false, false, false)
            })?;
        }
        if let Some(index) = step.and_then(|s| s.scroll_item) {
            with_cards(&widget, nav, |c| {
                let rect = c
                    .scroller
                    .item(index)
                    .ok_or(Error::Contract("card index"))?
                    .state()
                    .rect;
                c.scroller.scroll_to(
                    f64::from(rect.x + rect.width / 2.0) - 268.0,
                    false,
                    false,
                    false,
                )
            })?;
        }
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
                Action::Failure(e) => return Err(e.into()),
                _ => return Err("unexpected cards action".into()),
            }
        }
        context.params.flush()?;
        results.push(serde_json::json!({"effects":*effects.borrow(),"depth":stack.len(),"offset":with_cards(&widget,nav,|c|Ok(c.scroller.scroll_offset))?,"driver_view":context.params.boolean("IsDriverViewEnabled")?,"record_front":context.params.boolean("RecordFront")?}));
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
