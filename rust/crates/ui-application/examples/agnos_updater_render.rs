use openpilot_startup_ui::{config::Config, renderer::Renderer};
use openpilot_ui_application::{
    agnos_updater::{
        mici, tici,
        view::{Action, Shared},
        worker::{Layout, State},
    },
    context::Actions,
    mici::settings::network::wifi::WifiContext,
};
use openpilot_ui_framework::{
    canvas::Canvas,
    geometry::{MouseEvent, Rect},
    keys::KeyboardInput,
    navigation::NavWidget,
    network::{WifiBackend, WifiSession},
    scroller::Scroller,
    stack::NavigationStack,
    widget::{Frame, NavigationQueue, WidgetHandle},
    Error,
};
use openpilot_wifi::{Command, Event, Snapshot};
use serde::Deserialize;
use std::{cell::RefCell, path::Path, rc::Rc};
#[derive(Default)]
struct Backend {
    snapshot: Snapshot,
    events: Vec<Event>,
    commands: Vec<Command>,
}
#[derive(Clone)]
struct Fake(Rc<RefCell<Backend>>);
impl WifiBackend for Fake {
    fn snapshot(&self) -> Result<Snapshot, Error> {
        Ok(self.0.borrow().snapshot.clone())
    }
    fn drain_events(&self) -> Result<Vec<Event>, Error> {
        Ok(std::mem::take(&mut self.0.borrow_mut().events))
    }
    fn send(&self, c: Command) -> Result<(), Error> {
        self.0.borrow_mut().commands.push(c);
        Ok(())
    }
}
#[derive(Deserialize)]
struct Scene {
    config: Config,
    state: State,
    wifi: Snapshot,
    frames: u32,
    #[serde(default)]
    page: String,
    #[serde(default)]
    steps: Vec<Step>,
    #[serde(default)]
    capture_frames: Vec<u32>,
}
#[derive(Deserialize)]
struct Step {
    frame: u32,
    state: Option<State>,
    wifi: Option<Snapshot>,
    #[serde(default)]
    events: Vec<MouseEvent>,
    #[serde(default)]
    wifi_events: Vec<Event>,
    scroll: Option<f64>,
    #[serde(default)]
    connected: bool,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, scene, output] = args.as_slice() else {
        return Err("agnos_updater_render ROOT SCENE OUTPUT".into());
    };
    let scene: Scene = serde_json::from_slice(&std::fs::read(scene)?)?;
    let assets = Path::new(root).join("openpilot/selfdrive/assets");
    let renderer = Renderer::new(scene.config, &assets, false, "en")?;
    let mut canvas = Canvas::new(renderer, &assets);
    let backend = Rc::new(RefCell::new(Backend {
        snapshot: scene.wifi.clone(),
        ..Default::default()
    }));
    let session = WifiSession::new(Fake(backend.clone()))?;
    let actions = Actions::default();
    let now_clock = Rc::new(std::cell::Cell::new(0.0));
    let clock = now_clock.clone();
    let shared = Shared::new(scene.state.clone());
    let context = WifiContext {
        actions: actions.clone(),
        pc: true,
        now_monotonic: Rc::new(move || clock.get()),
    };
    let navigation = NavigationQueue::default();
    let mut stack = NavigationStack::default();
    stack.render_depth = if scene.config.large_viewport { 1 } else { 2 };
    let mut views = if scene.state.layout == Layout::Mici {
        Some(mici::Views::new(
            shared.clone(),
            context,
            session.clone(),
            &mut canvas,
        )?)
    } else {
        None
    };
    let tici = if scene.state.layout == Layout::Tici {
        let view = tici::View::new(
            shared.clone(),
            openpilot_ui_framework::network::Context {
                session: session.clone(),
                translate: Rc::new(str::to_owned),
            },
            &mut canvas,
        )?;
        view.wifi_screen.set(scene.page == "network");
        Some(WidgetHandle::new(view))
    } else {
        None
    };
    let mut results = Vec::new();
    let mut last = MouseEvent::default();
    let mut connected = false;
    for index in 0..scene.frames {
        let now = f64::from(index) / 20.0;
        now_clock.set(now);
        let step = scene.steps.iter().find(|s| s.frame == index);
        if let Some(step) = step {
            if let Some(s) = &step.state {
                *shared.state.borrow_mut() = s.clone();
            }
            if let Some(s) = &step.wifi {
                backend.borrow_mut().snapshot = s.clone();
            }
            backend.borrow_mut().events.extend(step.wifi_events.clone());
            connected = step.connected;
        }
        let events = step.map_or(&[][..], |s| s.events.as_slice());
        if let Some(e) = events.last() {
            last = *e;
        }
        let keyboard = KeyboardInput::default();
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
            last_event: last,
            cursor: last.pos,
            wheel: 0.0,
            show_touches: false,
        };
        if index == 0 {
            if let Some(tici) = &tici {
                stack.push(tici.clone(), &frame)?;
            }
            if let Some(v) = &views {
                stack.push(v.root.clone(), &frame)?;
                let page = match scene.page.as_str() {
                    "root" => None,
                    "network" => Some(v.network.clone()),
                    "failed" => None,
                    "wifi" => Some(v.wifi.clone()),
                    _ => Some(v.progress.clone()),
                };
                if let Some(page) = page {
                    stack.push(page, &frame)?;
                }
            }
        }
        if let Some(scroll) = step.and_then(|s| s.scroll) {
            if let Some(v) = &views {
                let h = stack.active().ok_or("no active widget")?;
                if h.same(&v.root) {
                    h.get_mut::<Scroller>()?
                        .scroll_to(scroll, false, false, false)?;
                } else {
                    let mut nav = h.get_mut::<NavWidget>()?;
                    if let Some(scroller) =
                        (nav.content.as_mut() as &mut dyn std::any::Any).downcast_mut::<Scroller>()
                    {
                        scroller.scroll_to(scroll, false, false, false)?;
                    } else if let Some(network) = (nav.content.as_mut() as &mut dyn std::any::Any)
                        .downcast_mut::<mici::Network>()
                    {
                        network.scroller.scroll_to(scroll, false, false, false)?;
                    }
                }
            }
        }
        canvas.renderer.begin();
        session.process()?;

        if let Some(v) = &mut views {
            v.failure(&shared.state.borrow(), &frame)?;
            v.network_tick(&frame, &stack, &session, connected)?;
            v.process_callbacks()?;
        }
        openpilot_ui_application::agnos_updater::runtime::dispatch_dialogs(
            &actions,
            &mut canvas,
            &navigation,
        )?;
        let mut effects = Vec::new();
        loop {
            let action = shared.actions.borrow_mut().pop_front();
            let Some(a) = action else { break };
            effects.push(format!("{a:?}"));
            if let Some(v) = &views {
                match a {
                    Action::Network => v.go(v.network.clone(), &frame),
                    Action::Progress => v.go(v.progress.clone(), &frame),
                    _ => {}
                }
            }
        }
        stack.process(&frame)?;
        stack.render(
            &frame,
            Rect {
                x: 0.0,
                y: 0.0,
                width: scene.config.width(),
                height: scene.config.height(),
            },
            &mut canvas,
        )?;
        openpilot_ui_application::agnos_updater::runtime::dispatch_dialogs(
            &actions,
            &mut canvas,
            &navigation,
        )?;
        stack.process(&frame)?;
        if scene.capture_frames.contains(&index) {
            canvas
                .renderer
                .screenshot(&Path::new(output).with_file_name(format!(
                    "{}-frame-{index:04}.png",
                    Path::new(output)
                        .file_stem()
                        .ok_or("no stem")?
                        .to_string_lossy()
                )))?;
        }
        results.push(serde_json::json!({"effects":effects,"commands":backend.borrow().commands,"state":*shared.state.borrow(),"stack":stack.len()}));
        if index + 1 == scene.frames {
            canvas.renderer.screenshot(Path::new(output))?;
        }
        canvas.renderer.end();
    }
    std::fs::write(
        Path::new(output).with_extension("json"),
        serde_json::to_vec(&results)?,
    )?;
    Ok(())
}
