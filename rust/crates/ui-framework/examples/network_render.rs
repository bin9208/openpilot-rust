use openpilot_startup_ui::{
    config::Config,
    renderer::{Renderer, TextureOptions},
};
use openpilot_ui_framework::{
    canvas::Canvas,
    geometry::{MouseEvent, Point, Rect},
    keys::KeyboardInput,
    network::{
        AdvancedNetworkSettings, Context, NetworkUi, WifiBackend, WifiManagerUi, WifiSession,
    },
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
    fn send(&self, command: Command) -> Result<(), Error> {
        self.0.borrow_mut().commands.push(command);
        Ok(())
    }
}
#[derive(Deserialize)]
struct Scene {
    kind: String,
    snapshot: Snapshot,
    frames: Vec<Input>,
}
#[derive(Deserialize)]
struct Input {
    #[serde(default)]
    events: Vec<Event>,
    #[serde(default)]
    snapshot: Option<Snapshot>,
    #[serde(default)]
    touch: Vec<MouseEvent>,
    #[serde(default)]
    operation: String,
    #[serde(default)]
    ssid: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    chars: String,
    #[serde(default)]
    wheel: f64,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, scene, output] = args.as_slice() else {
        return Err("network_render ROOT SCENE OUTPUT".into());
    };
    let scene: Scene = serde_json::from_slice(&std::fs::read(scene)?)?;
    let config = Config {
        big: true,
        large_viewport: true,
        pc: true,
        scale: 1.0,
    };
    let assets = Path::new(root).join("openpilot/selfdrive/assets");
    let renderer = Renderer::new(config, &assets, false, "en")?;
    let mut canvas = Canvas::new(renderer, &assets);
    let backend = Rc::new(RefCell::new(Backend {
        snapshot: scene.snapshot,
        ..Default::default()
    }));
    let session = WifiSession::new(Fake(backend.clone()))?;
    let context = Context {
        session,
        translate: Rc::new(str::to_owned),
    };
    let params = Rc::new(openpilot_params::Params::open(
        &Path::new(output).with_extension("params"),
        "d",
    )?);
    let mut texture = |path: &str, (width, height)| {
        canvas.texture(
            path,
            TextureOptions {
                width: Some(width),
                height: Some(height),
                ..Default::default()
            },
        )
    };
    let widget = match scene.kind.as_str() {
        "wifi" => WidgetHandle::new(WifiManagerUi::new(context, &mut texture)?),
        "advanced" => WidgetHandle::new(AdvancedNetworkSettings::new(
            context,
            params.clone(),
            &mut texture,
        )?),
        "network" => WidgetHandle::new(NetworkUi::new(context, params.clone(), &mut texture)?),
        _ => return Err("unknown network scene".into()),
    };
    let navigation = NavigationQueue::default();
    let mut stack = NavigationStack::default();
    stack.render_depth = 1;
    let mut results = Vec::new();
    let mut last = MouseEvent::default();
    for (index, input) in scene.frames.iter().enumerate() {
        use num_traits::ToPrimitive;
        let now = index.to_f64().ok_or("frame overflow")? / 20.0;
        if let Some(snapshot) = &input.snapshot {
            backend.borrow_mut().snapshot = snapshot.clone();
        }
        backend.borrow_mut().events.extend(input.events.clone());
        if let Some(event) = input.touch.last() {
            last = *event;
        }
        let keyboard = KeyboardInput::default();
        keyboard
            .characters
            .borrow_mut()
            .extend(input.chars.chars().map(u32::from));
        let frame = Frame {
            index: 0,
            now,
            monotonic: now,
            keyboard: &keyboard,
            navigation: &navigation,
            dt: 0.05,
            target_fps: 20.0,
            awake: true,
            events: &input.touch,
            last_event: last,
            cursor: Point::default(),
            wheel: input.wheel,
            show_touches: false,
        };
        if index == 0 {
            stack.push(widget.clone(), &frame)?;
        }
        if scene.kind == "wifi" {
            let ui = widget.get_mut::<WifiManagerUi>()?;
            match input.operation.as_str() {
                "choose" | "forget" => {
                    let network = ui
                        .networks()
                        .iter()
                        .find(|network| network.ssid == input.ssid)
                        .ok_or("fixture network absent")?
                        .clone();
                    if input.operation == "choose" {
                        ui.model.borrow_mut().choose(&network);
                    } else {
                        ui.model.borrow_mut().request_forget(&network);
                    }
                }
                "password" => {
                    let network = ui
                        .model
                        .borrow()
                        .network
                        .clone()
                        .ok_or("auth network absent")?;
                    ui.model.borrow_mut().password(
                        &network,
                        openpilot_ui_framework::widget::DialogResult::Confirm,
                        &input.text,
                    );
                }
                "forgot" => {
                    let network = ui
                        .model
                        .borrow()
                        .network
                        .clone()
                        .ok_or("forget network absent")?;
                    ui.model.borrow_mut().forgot_result(
                        &network,
                        openpilot_ui_framework::widget::DialogResult::Confirm,
                    );
                }
                "" => {}
                _ => return Err("unknown network action".into()),
            }
        }
        canvas.renderer.begin();
        stack.render(
            &frame,
            Rect {
                x: 0.0,
                y: 0.0,
                width: 2160.0,
                height: 1080.0,
            },
            &mut canvas,
        )?;
        let state = if scene.kind == "wifi" {
            let ui = widget.get::<WifiManagerUi>()?;
            let model = ui.model.borrow();
            serde_json::json!({"phase":model.phase,"network":model.network.as_ref().map(|network|&network.ssid),"retry":model.password_retry,"ip":ui.ip_address,"keyboard":ui.keyboard.get::<openpilot_ui_framework::keyboard::Keyboard>()?.text()})
        } else {
            serde_json::Value::Null
        };
        results.push(serde_json::json!({"state":state,"stack":stack.len(),"commands":backend.borrow().commands,"apn":params.get("GsmApn")?.map(|bytes|String::from_utf8_lossy(&bytes).into_owned()),"roaming":params.get_bool("GsmRoaming")?,"metered":params.get_bool("GsmMetered")?}));
        if index + 1 == scene.frames.len() {
            canvas.renderer.screenshot(Path::new(output))?;
        }
        canvas.renderer.end();
    }
    std::fs::write(
        Path::new(output).with_extension("json"),
        serde_json::to_vec_pretty(&results)?,
    )?;
    Ok(())
}
