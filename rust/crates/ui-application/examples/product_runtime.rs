#[path = "support/runtime_context.rs"]
mod fixture;
#[path = "support/runtime_probe.rs"]
mod probe;
use openpilot_ui_application::{
    context::{Action, Page, Panel},
    params::Read,
    root_layout::Main,
    runtime::Runtime,
};
use openpilot_ui_framework::{
    application::{Application, ApplicationConfig},
    Error,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    io::BufRead,
    path::{Path, PathBuf},
    rc::Rc,
    sync::mpsc,
};
#[derive(Default, Deserialize)]
struct Command {
    #[serde(default)]
    params: BTreeMap<String, Option<String>>,
    #[serde(default)]
    memory: BTreeMap<String, Option<String>>,
    page: Option<String>,
    alert: Option<String>,
    capture: Option<String>,
    asleep: Option<bool>,
    dialog: Option<String>,
    prime: Option<i32>,
    timeout: Option<i32>,
    #[serde(default)]
    bookmark: bool,
    #[serde(default)]
    fail: bool,
    #[serde(default)]
    close: bool,
}
fn page(name: &str) -> Result<Page, Error> {
    Ok(match name {
        "home" => Page::Home,
        "device" => Page::Settings(Panel::Device),
        "toggles" => Page::Settings(Panel::Toggles),
        "network" => Page::Settings(Panel::Network),
        "software" => Page::Settings(Panel::Software),
        "developer" => Page::Settings(Panel::Developer),
        "firehose" => Page::Settings(Panel::Firehose),
        "egpu" => Page::Settings(Panel::Egpu),
        "web" => Page::CarrotWeb,
        "pairing" => Page::Pairing,
        "driver" => Page::DriverCamera,
        "training" => Page::Training,
        "terms" => Page::Terms,
        "regulatory" => Page::Regulatory,
        "language" => Page::Language,
        _ => return Err(Error::Contract("unknown runtime fixture page")),
    })
}
fn dialog(
    name: &str,
    context: &openpilot_ui_application::context::Context,
    canvas: &mut openpilot_ui_framework::canvas::Canvas,
) -> Result<(), openpilot_ui_application::Error> {
    use openpilot_ui_application::context::{actions::Selection, Confirmation};
    use openpilot_ui_framework::{callback::Callback, widget::DialogResult};
    let source = context.clone();
    let callback = Callback::new(move |value: DialogResult| {
        if let Err(error) = source
            .params
            .put_bool("ExperimentalModeConfirmed", value == DialogResult::Confirm)
        {
            source.actions.push(Action::Failure(error));
        }
    });
    let action = match name {
        "select" => {
            let source = context.clone();
            Action::Select(Selection {
                title: "Fixture selection".into(),
                options: vec!["alpha".into(), "beta".into()],
                selected: "alpha".into(),
                callback: Callback::new(move |value: Option<String>| {
                    if let Some(value) = value {
                        if let Err(error) = source.params.put("GithubUsername", value.as_bytes()) {
                            source.actions.push(Action::Failure(error));
                        }
                    }
                }),
            })
        }
        "confirm" => Action::Confirm(Confirmation {
            text: "Confirm native action".into(),
            confirm: "Confirm".into(),
            cancel: "Cancel".into(),
            rich: false,
            callback,
        }),
        "input" => {
            let source = context.clone();
            Action::MiciInput(
                openpilot_ui_application::mici::widgets::dialog::InputOptions {
                    hint: "Fixture input".into(),
                    text: "a".into(),
                    minimum_length: 1,
                    auto_return: String::new(),
                    callback: Some(Rc::new(move |value| {
                        if let Err(error) = source.params.put("GithubUsername", value.as_bytes()) {
                            source.actions.push(Action::Failure(error));
                        }
                    })),
                },
            )
        }
        "slider" => {
            let source = context.clone();
            Action::MiciConfirm(
                openpilot_ui_application::mici::widgets::dialog::Confirmation {
                    title: "slide to\nconfirm".into(),
                    icon: openpilot_ui_application::paint::texture(
                        canvas,
                        "icons_mici/settings/device/reboot.png",
                        (64, 64),
                    )?,
                    red: false,
                    exit_on_confirm: true,
                    callback: Rc::new(move || {
                        if let Err(error) =
                            source.params.put_bool("ExperimentalModeConfirmed", true)
                        {
                            source.actions.push(Action::Failure(error));
                        }
                    }),
                },
            )
        }
        _ => {
            return Err(openpilot_ui_application::Error::Contract(
                "unknown fixture dialog",
            ));
        }
    };
    context.actions.push(action);
    Ok(())
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, output] = args.as_slice() else {
        return Err("product_runtime ROOT OUTPUT".into());
    };
    let root = Path::new(root);
    let output = Path::new(output);
    std::fs::create_dir_all(output)?;
    let app = Application::new(ApplicationConfig::for_runtime(
        root,
        "Native product UI fixture",
    )?)?;
    let (context, resources) = fixture::context(root, output, &app)?;
    let hardware = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = Runtime::new(
        app,
        context.clone(),
        resources,
        Box::new(fixture::Hardware(hardware.clone())),
        "rustvision",
        PathBuf::from("/missing/openpilot-updated"),
    )?;
    runtime.app.diagnostics.record_dir = output.join("videos");
    let (sender, receiver) = mpsc::channel();
    let input = std::thread::Builder::new()
        .name("ui-fixture-input".into())
        .spawn(move || {
            for line in std::io::stdin().lock().lines() {
                let command = line.map_err(|error| error.to_string()).and_then(|line| {
                    serde_json::from_str::<Command>(&line).map_err(|error| error.to_string())
                });
                if sender.send(command).is_err() {
                    break;
                }
            }
        })?;
    let mut snapshots = Vec::new();
    let mut asleep = false;
    let mut capture = None;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        while !runtime.app.is_closed() {
            for command in receiver.try_iter() {
                let command = command.map_err(std::io::Error::other)?;
                for (store, values) in [
                    (context.params.as_ref(), command.params),
                    (context.memory.as_ref(), command.memory),
                ] {
                    for (key, value) in values {
                        if let Some(value) = value {
                            store.put(&key, value.as_bytes())?;
                        } else {
                            store.remove(&key)?;
                        }
                    }
                }
                if let Some(page_name) = command.page {
                    context.open(page(&page_name)?);
                }
                if let Some(name) = command.dialog {
                    dialog(&name, &context, &mut runtime.app.canvas)?;
                }
                if let Some(prime) = command.prime {
                    context.prime.set(prime);
                }
                if let Some(timeout) = command.timeout {
                    context
                        .actions
                        .push(Action::SetInteractiveTimeout(Some(timeout)));
                }
                if let Some(text) = command.alert {
                    if context.big {
                        context.actions.push(Action::Alert(text));
                    } else {
                        context.actions.push(Action::MiciAlert {
                            title: "fixture".into(),
                            description: text,
                        });
                    }
                }
                if command.bookmark {
                    context.actions.push(Action::Bookmark);
                }
                if command.fail {
                    context.actions.push(Action::Failure(
                        openpilot_ui_application::Error::Contract("injected runtime failure"),
                    ));
                }
                if command.close {
                    context.actions.push(Action::Exit);
                }
                if let Some(value) = command.asleep {
                    asleep = value;
                }
                if let Some(value) = command.capture {
                    capture = Some(value);
                }
            }
            runtime.app.should_render = !asleep;
            let rendered = runtime.step()?;
            let root = runtime.root.get::<Main>()?.snapshot();
            let navigation_y = runtime.app.stack.active().and_then(|widget| {
                widget
                    .get::<openpilot_ui_framework::navigation::NavWidget>()
                    .ok()
                    .map(|widget| widget.state.rect.y)
            });
            let current = json!({"rendered":rendered, "draw_frame":runtime.app.frame_count(), "message_frame":context.messages.borrow().state.frame(),
                "root":root,"stack":runtime.app.stack.len(),"navigation_y":navigation_y,"started":context.ui.borrow().started,"engaged":context.ui.borrow().engaged,
                "language":context.translations.update(|catalog|catalog.language().to_owned()),"driver_view":context.params.boolean("IsDriverViewEnabled")?,
                "ScreenRecord":context.params.boolean("ScreenRecord")?,"hardware":*hardware.borrow(),"recording_child":runtime.app.diagnostics.recording_child_pid()});
            let mut current = current;
            current["callback_value"] = json!(context.params.string("GithubUsername")?);
            current["callback_confirmed"] =
                json!(context.params.boolean("ExperimentalModeConfirmed")?);
            current["active"] = probe::active(runtime.app.stack.active().as_ref())?;
            if let Some(name) = capture.take() {
                if !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
                {
                    return Err("invalid fixture capture name".into());
                }
                runtime
                    .app
                    .canvas
                    .renderer
                    .screenshot(&output.join(format!("{name}.png")))?;
                std::fs::write(
                    output.join(format!("{name}.json")),
                    serde_json::to_vec_pretty(&current)?,
                )?;
            }
            println!("RUNTIME_FRAME {current}");
            snapshots.push(current);
        }
        Ok(())
    })();
    let close = runtime.close();
    std::fs::write(
        output.join("frames.json"),
        serde_json::to_vec_pretty(&snapshots)?,
    )?;
    std::fs::write(
        output.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"error":result.as_ref().err().map(ToString::to_string),"close_error":close.as_ref().err().map(ToString::to_string),"recording":runtime.app.diagnostics.is_recording(),"child":runtime.app.diagnostics.recording_child_pid(),"frames":snapshots.len()}),
        )?,
    )?;
    drop(runtime);
    if input.is_finished() {
        input.join().map_err(|_| "fixture input worker panicked")?;
    }
    result?;
    close?;
    Ok(())
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("native product fixture stopped: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
