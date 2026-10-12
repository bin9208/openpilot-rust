use openpilot_cluster::replay_tools::{
    self, Fonts, GuiConfig, InputState, Layout, RouteReplayToolsWindow, ScaleBackend, ToolsState,
    UiAction, WindowConfig,
};
use openpilot_sdl2::gui::{Context, Event};
use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
#[derive(serde::Deserialize)]
struct RenderCase {
    name: String,
    width: i32,
    height: i32,
    state: ToolsState,
    video_file: Option<PathBuf>,
    #[serde(default)]
    generic: bool,
}
#[derive(serde::Deserialize)]
struct EventCase {
    width: i32,
    height: i32,
    state: ToolsState,
    events: Vec<RawEvent>,
}
#[derive(serde::Deserialize)]
struct RawEvent {
    kind: String,
    #[serde(default)]
    key: i32,
    #[serde(default)]
    repeat: bool,
    #[serde(default)]
    x: i32,
    #[serde(default)]
    y: i32,
    #[serde(default)]
    button: u8,
}
fn event(raw: &RawEvent) -> Result<Event, Box<dyn std::error::Error>> {
    Ok(match raw.kind.as_str() {
        "quit" => Event::Quit,
        "key" => Event::KeyDown {
            key: raw.key,
            repeat: raw.repeat,
        },
        "down" => Event::MouseDown {
            x: raw.x,
            y: raw.y,
            button: raw.button,
        },
        "up" => Event::MouseUp {
            x: raw.x,
            y: raw.y,
            button: raw.button,
        },
        "move" => Event::MouseMove { x: raw.x, y: raw.y },
        "other" => Event::Other(0),
        value => return Err(format!("unknown event {value}").into()),
    })
}
fn render(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let cases: Vec<RenderCase> = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    let output = Path::new(&args[3]);
    std::fs::create_dir_all(output)?;
    let config: GuiConfig = serde_json::from_slice(&std::fs::read(&args[4])?)?;
    let context = Context::load(&config.sdl_library, &config.ttf_library)?;
    let fonts = Fonts::load(&context, &config.fonts)?;
    for mut case in cases {
        if let Some(path) = case.video_file {
            case.state.video_rgba = Some(std::fs::read(path)?);
        }
        let mut window = context.window(
            "Carrot Cluster Replay Tools",
            case.width,
            case.height,
            (0, 0),
            true,
        )?;
        let image = replay_tools::render(
            &case.state,
            &fonts,
            case.width,
            case.height,
            if case.generic {
                ScaleBackend::Generic
            } else {
                ScaleBackend::default()
            },
        )?;
        window.present(&image.rgba, image.width, image.height)?;
        std::fs::write(
            output.join(format!("{}.rgba", case.name)),
            window.capture()?.rgba,
        )?;
        std::fs::write(
            output.join(format!("{}.canvas.rgba", case.name)),
            image.rgba,
        )?;
    }
    Ok(())
}
fn events(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let cases: Vec<EventCase> = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    let mut output = Vec::new();
    for mut case in cases {
        let layout = Layout::new(case.width, case.height)?;
        let mut input = InputState::default();
        let mut commands = Vec::new();
        let mut chooser = 0;
        for raw in case.events {
            if let Some(action) = input.handle(event(&raw)?, &layout, &mut case.state) {
                match action {
                    UiAction::Command(command) => {
                        if commands.len() < 16 {
                            commands.push(command);
                        }
                    }
                    UiAction::ChooseLog => chooser += 1,
                }
            }
        }
        output.push(serde_json::json!({"commands":commands,"chooser":chooser,"state":case.state,"dragging":input.dragging_seek()}));
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
fn parent(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let config: GuiConfig = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    let launcher = PathBuf::from(&args[3]);
    let mut window = RouteReplayToolsWindow::new(&WindowConfig {
        program: std::env::current_exe()?,
        launcher,
        cwd: std::env::current_dir()?,
        gui: config,
    })?;
    println!(
        "{}",
        serde_json::json!({"started":window.process_id(),"directory":window.socket_directory()})
    );
    std::io::stdout().flush()?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            match line {
                Ok(line) => {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    eprintln!("fixture stdin: {error}");
                    break;
                }
            }
        }
    });
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline && window.is_alive()? {
        for line in rx.try_iter() {
            let value: serde_json::Value = serde_json::from_str(&line)?;
            if value.get("close").is_some() {
                window.close()?;
                println!(
                    "{}",
                    serde_json::json!({"closed":true,"exit":format!("{:?}",window.exit_status())})
                );
                return Ok(());
            }
            let mut state: ToolsState = serde_json::from_value(value.clone())?;
            if let Some(file) = value.get("video_file").and_then(serde_json::Value::as_str) {
                state.video_rgba = Some(std::fs::read(file)?);
            }
            window.update(state)?;
        }
        if let Some(action) = window.poll() {
            println!("{}", serde_json::to_string(&action)?);
            std::io::stdout().flush()?;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    if let Some(action) = window.poll() {
        println!("{}", serde_json::to_string(&action)?);
    }
    window.close()?;
    println!(
        "{}",
        serde_json::json!({"closed":true,"exit":format!("{:?}",window.exit_status())})
    );
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str){Some("--replay-tools-helper") if args.len()==4=>Ok(replay_tools::run_helper(Path::new(&args[2]),&args[3])?),Some("render") if args.len()==5=>render(&args),Some("events") if args.len()==3=>events(&args),Some("parent") if args.len()==4=>parent(&args),_=>Err("expected render CASES OUTPUT CONFIG, events CASES, parent CONFIG LAUNCHER or --replay-tools-helper SOCKET CONFIG".into())}
}
