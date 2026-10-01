use num_traits::ToPrimitive;
use openpilot_startup_ui::{
    config::Config,
    renderer::{Renderer, TextureOptions},
};
use openpilot_ui_framework::{
    canvas::Canvas,
    dialog::{ConfirmDialog, MultiOptionDialog},
    geometry::{MouseEvent, Point, Rect},
    html::HtmlRenderer,
    inputbox::InputBox,
    keyboard::{Keyboard, KeyboardOptions},
    keys::KeyboardInput,
    list::{ButtonAction, ListItem, MultipleButtonAction, TextAction, ToggleAction},
    mici_keyboard::MiciKeyboard,
    slider::{Slider, SliderAssets},
    widget::{Frame, NavigationQueue, NavigationRequest, Widget},
};
use serde::Deserialize;
use std::{cell::Cell, path::Path, rc::Rc};
#[path = "support/forms.rs"]
mod forms;
use forms::{flag, number, string, Form, Scene};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, scene, output] = args.as_slice() else {
        return Err("form_render ROOT SCENE OUTPUT".into());
    };
    let scene: Scene = serde_json::from_slice(&std::fs::read(scene)?)?;
    let assets = Path::new(root).join("openpilot/selfdrive/assets");
    let renderer = Renderer::new(scene.config, &assets, false, &scene.language)?;
    let mut canvas = Canvas::new(renderer, &assets);
    let p = &scene.props;
    let mut form = match scene.kind.as_str() {
        "mici" => {
            let mut v = MiciKeyboard::new(20.0, |path, (width, height)| {
                canvas.texture(
                    path,
                    TextureOptions {
                        width: Some(width),
                        height: Some(height),
                        keep_aspect: !path.ends_with("keyboard_background.png"),
                        ..Default::default()
                    },
                )
            })?;
            v.text = scene.text.clone();
            v.auto_return = string(p, "auto_return", "").into();
            Form::Mici(Box::new(v))
        }
        "input" => {
            let mut v = InputBox::new(
                usize::try_from(number(p, "max", 255.0).to_u64().ok_or("invalid max")?)?,
                flag(p, "password"),
            );
            v.set_text(&scene.text, &canvas, 0.0);
            Form::Input(Box::new(v))
        }
        "html" => {
            let mut v = HtmlRenderer::new(&scene.text, number(p, "size", 48.0))?;
            v.center = flag(p, "center");
            Form::Html(Box::new(v))
        }
        "keyboard" => {
            let mut v = Keyboard::new(
                KeyboardOptions {
                    min_length: 8,
                    password: flag(p, "password"),
                    password_toggle: flag(p, "toggle"),
                    ..Default::default()
                },
                |path, (width, height)| {
                    canvas.texture(
                        path,
                        TextureOptions {
                            width: Some(width),
                            height: Some(height),
                            ..Default::default()
                        },
                    )
                },
            )?;
            v.set_title("Keyboard Input", "Type your text below");
            v.set_text(&scene.text);
            Form::Keyboard(Box::new(v))
        }
        "confirm" => {
            let mut v = ConfirmDialog::new(&scene.text, "Continue", string(p, "cancel", "Cancel"))?;
            v.rich = flag(p, "rich");
            Form::Confirm(Box::new(v))
        }
        "options" => Form::Options(Box::new(MultiOptionDialog::new(
            &scene.text,
            vec![
                "First".into(),
                "Second".into(),
                "Third".into(),
                "Fourth".into(),
            ],
            "First",
        ))),
        "list" => {
            let mut v = ListItem::new(&scene.text)?;
            v.description = string(p, "description", "").to_owned().into();
            v.description_visible = flag(p, "description_visible");
            v.action = Some(match string(p, "action", "toggle") {
                "toggle" => Box::new(ToggleAction::new(true)),
                "button" => {
                    let mut b = ButtonAction::new("Edit");
                    b.value = "Current".to_owned().into();
                    Box::new(b)
                }
                "text" => Box::new(TextAction::new(
                    "Connected",
                    u32::from_le_bytes([170, 170, 170, 255]),
                )),
                "multiple" => Box::new(MultipleButtonAction::new(
                    vec!["One".to_owned().into(), "Two".to_owned().into()],
                    200.0,
                    0,
                )),
                _ => return Err("unknown list action".into()),
            });
            Form::List(Box::new(v))
        }
        "slider" => Form::Slider(Box::new(Slider::new(
            &scene.text,
            SliderAssets::larger(&mut canvas, flag(p, "green"))?,
            20.0,
            false,
        ))),
        _ => return Err("unknown form kind".into()),
    };
    form.widget().set_rect(scene.rect);
    form.widget().set_parent_rect(scene.rect);
    let navigation = NavigationQueue::default();
    let mut results = Vec::new();
    let mut pops = 0;
    let confirmed = Rc::new(Cell::new(0));
    if let Form::Slider(v) = &mut form {
        let count = confirmed.clone();
        v.on_confirm = Some(Box::new(move || count.set(count.get() + 1)));
    }
    for index in 0..scene.frames {
        let now = index.to_f64().ok_or("frame overflow")? / 20.0;
        let keyboard = KeyboardInput::default();
        let mut keyboard = keyboard;
        let mut events = Vec::new();
        for action in scene.actions.iter().filter(|a| a.frame == index) {
            if action.key != 0 {
                keyboard.queued.borrow_mut().push_back(action.key);
                keyboard.pressed.insert(action.key);
            }
            keyboard.down.extend(&action.down);
            events.extend(&action.events);
            match action.operation.as_str() {
                "character" => keyboard
                    .characters
                    .borrow_mut()
                    .extend(action.text.chars().map(u32::from)),
                "key" => {
                    if let Form::Keyboard(v) = &mut form {
                        v.key(&action.text, &canvas, now);
                    }
                }
                "text" => match &mut form {
                    Form::Input(v) => v.set_text(&action.text, &canvas, now),
                    Form::Keyboard(v) => v.set_text(&action.text),
                    Form::Html(v) => v.parse(&action.text)?,
                    Form::Confirm(v) => v.set_text(&action.text)?,
                    _ => {}
                },
                "cursor" => {
                    if let Form::Input(v) = &mut form {
                        v.set_cursor(action.value, &canvas, now);
                    }
                }
                "selection" => {
                    if let Form::Options(v) = &mut form {
                        *v.selection.borrow_mut() = action.text.clone();
                    }
                }
                "backspace" => {
                    if let Form::Mici(v) = &mut form {
                        v.backspace();
                    }
                }
                "space" => {
                    if let Form::Mici(v) = &mut form {
                        v.space();
                    }
                }
                "clear" => match &mut form {
                    Form::Input(v) => v.clear(),
                    Form::Keyboard(v) => v.clear(),
                    _ => {}
                },
                "" => {}
                _ => return Err("unknown form operation".into()),
            }
        }
        let frame = Frame {
            now,
            monotonic: now,
            keyboard: &keyboard,
            navigation: &navigation,
            dt: 0.05,
            target_fps: 20.0,
            awake: true,
            last_event: events.last().copied().unwrap_or_default(),
            cursor: events.last().map_or(Point::default(), |event| event.pos),
            events: &events,
            wheel: 0.0,
            show_touches: flag(p, "show_touches"),
        };
        canvas.renderer.begin();
        form.widget().render(&frame, &mut canvas)?;
        while let Some(request) = navigation.pop() {
            match request {
                NavigationRequest::Pop(callback) => {
                    pops += 1;
                    if let Some(callback) = callback {
                        callback();
                    }
                }
                _ => return Err("unexpected navigation request".into()),
            }
        }
        results.push(serde_json::json!({"state":form.snapshot(&canvas,now),"pops":pops,"confirmed":confirmed.get()}));
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
