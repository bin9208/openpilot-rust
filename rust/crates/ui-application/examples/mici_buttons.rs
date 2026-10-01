use num_traits::ToPrimitive;
use openpilot_startup_ui::{
    config::Config,
    renderer::{Renderer, TextureOptions},
};
use openpilot_ui_application::mici::widgets::{
    big_button::{BigButton, Kind},
    circle_button::CircleButton,
};
use openpilot_ui_framework::{
    callback::Callback,
    canvas::Canvas,
    geometry::{MouseEvent, Point, Rect},
    keys::KeyboardInput,
    widget::{Frame, NavigationQueue, Widget},
};
use serde::Deserialize;
use std::{cell::RefCell, path::Path, rc::Rc};
#[derive(Deserialize)]
struct Scene {
    kind: String,
    text: String,
    value: String,
    language: String,
    frames: usize,
    actions: Vec<Action>,
}
#[derive(Deserialize)]
struct Action {
    frame: usize,
    #[serde(default)]
    operation: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    events: Vec<MouseEvent>,
}
enum Control {
    Big(Box<BigButton>),
    Circle(Box<CircleButton>),
}
impl Control {
    fn widget(&mut self) -> &mut dyn Widget {
        match self {
            Self::Big(v) => v.as_mut(),
            Self::Circle(v) => v.as_mut(),
        }
    }
    fn snapshot(&self) -> serde_json::Value {
        match self {
            Self::Big(v) => {
                serde_json::json!({"checked":v.checked(),"value":v.value,"scale":v.scale.position.x,"pressed":v.state.is_pressed()})
            }
            Self::Circle(v) => {
                serde_json::json!({"checked":v.checked,"value":"","scale":v.scale.position.x,"pressed":v.state.is_pressed()})
            }
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, scene, output] = args.as_slice() else {
        return Err("mici_buttons ROOT SCENE OUTPUT".into());
    };
    let scene: Scene = serde_json::from_slice(&std::fs::read(scene)?)?;
    let assets = Path::new(root).join("openpilot/selfdrive/assets");
    let renderer = Renderer::new(
        Config {
            big: false,
            large_viewport: false,
            pc: true,
            scale: 1.0,
        },
        &assets,
        false,
        &scene.language,
    )?;
    let mut canvas = Canvas::new(renderer, &assets);
    let mut load = |path: &str, (width, height)| {
        canvas.texture(
            path,
            TextureOptions {
                width: Some(width),
                height: Some(height),
                ..Default::default()
            },
        )
    };
    let icon = load(
        "icons_mici/settings/network/wifi_strength_full.png",
        (50, 37),
    )?;
    let calls = Rc::new(RefCell::new(Vec::<serde_json::Value>::new()));
    let mut control = if scene.kind.starts_with("circle") {
        let mut button = CircleButton::new(icon, &mut load)?;
        button.red = scene.kind == "circle-red";
        if scene.kind == "circle-toggle" {
            button.enable_toggle(&mut load)?;
        }
        let calls = calls.clone();
        button.changed = Some(Callback::new(move |v| {
            calls.borrow_mut().push(serde_json::json!({"toggle":v}))
        }));
        Control::Circle(Box::new(button))
    } else {
        let mut button = BigButton::new(&scene.text, &mut load)?;
        button.value = scene.value.clone();
        match scene.kind.as_str() {
            "toggle" => button.kind = Kind::Toggle(false),
            "multiple" => {
                button.set_multiple(vec!["First".into(), "Second".into(), "Third".into()])?
            }
            "grey" => button.set_grey(),
            "scroll" => {
                button.scroll = true;
                button.icon = Some(icon);
            }
            "button" => button.icon = Some(icon),
            _ => return Err("unknown kind".into()),
        }
        let changed = calls.clone();
        button.changed = Some(Callback::new(move |v| {
            changed.borrow_mut().push(serde_json::json!({"toggle":v}))
        }));
        let selected = calls.clone();
        button.selected = Some(Callback::new(move |v| {
            selected.borrow_mut().push(serde_json::json!({"select":v}))
        }));
        Control::Big(Box::new(button))
    };
    let size = control.widget().state().rect;
    let rect = Rect {
        x: (536.0 - size.width) / 2.0,
        y: 30.0,
        ..size
    };
    control.widget().set_rect(rect);
    control.widget().set_parent_rect(rect);
    let keyboard = KeyboardInput::default();
    let navigation = NavigationQueue::default();
    let mut results = Vec::new();
    for index in 0..scene.frames {
        let now = index.to_f64().ok_or("frame overflow")? / 20.0;
        let mut events = Vec::new();
        for action in scene.actions.iter().filter(|v| v.frame == index) {
            events.extend(&action.events);
            match action.operation.as_str() {
                "disable" => control.widget().state_mut().enabled = false.into(),
                "enable" => control.widget().state_mut().enabled = true.into(),
                "grow" => {
                    if let Control::Big(v) = &mut control {
                        v.grow_until = Some(now + 0.65)
                    }
                }
                "shake" => {
                    if let Control::Big(v) = &mut control {
                        v.shake_start = Some(now)
                    }
                }
                "rotate" => {
                    if let Control::Big(v) = &mut control {
                        v.set_rotate(true, now)
                    }
                }
                "text" => {
                    if let Control::Big(v) = &mut control {
                        v.text = action.text.clone()
                    }
                }
                "value" => {
                    if let Control::Big(v) = &mut control {
                        v.value = action.text.clone()
                    }
                }
                "" => {}
                _ => return Err("unknown operation".into()),
            }
        }
        control.widget().set_position(rect.x, rect.y);
        let frame = Frame {
            now,
            monotonic: now,
            keyboard: &keyboard,
            navigation: &navigation,
            dt: 0.05,
            target_fps: 20.0,
            awake: true,
            events: &events,
            last_event: events.last().copied().unwrap_or_default(),
            cursor: events.last().map_or(Point::default(), |v| v.pos),
            wheel: 0.0,
            show_touches: false,
        };
        canvas.renderer.begin();
        control.widget().render(&frame, &mut canvas)?;
        results.push(serde_json::json!({"state":control.snapshot(),"calls":*calls.borrow()}));
        if index + 1 == scene.frames {
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
