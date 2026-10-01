use crate::{
    config::Config, geometry::MouseEvent, input::Mouse, renderer::Renderer, spinner::Spinner,
    text_window::TextWindow, Error,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
#[derive(Clone, Copy)]
pub enum Kind {
    Spinner,
    Text,
}
pub fn run(kind: Kind) -> Result<(), Error> {
    let mut args = std::env::args().skip(1);
    let mut root = std::env::current_dir()?;
    let mut text = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--source-root" => {
                root = PathBuf::from(args.next().ok_or(Error::Contract("missing source root"))?)
            }
            "--text" => text = Some(args.next().ok_or(Error::Contract("missing text"))?),
            _ => {
                if text.is_some() {
                    return Err(Error::Contract("unexpected UI argument"));
                }
                text = Some(arg);
            }
        }
    }
    if root.ends_with("openpilot/system/ui") {
        for _ in 0..3 {
            root.pop();
        }
    }
    let (config, device) = Config::for_runtime()?;
    let pc = config.pc;
    let language = openpilot_params::Params::for_runtime()
        .ok()
        .and_then(|params| params.get("LanguageSetting").ok().flatten())
        .and_then(|value| String::from_utf8(value).ok())
        .unwrap_or_else(|| "en".into())
        .trim_start_matches("main_")
        .to_owned();
    let default_text="This is a sample text that will be wrapped and scrolled if necessary.\n            The text is long enough to demonstrate scrolling and word wrapping.".repeat(30);
    let startup_started = std::time::Instant::now();
    let mut renderer = Renderer::new(
        config,
        &root.join("openpilot/selfdrive/assets"),
        matches!(kind, Kind::Spinner),
        &language,
    )?;
    let mut diagnostics = crate::diagnostics::Diagnostics::new(
        &mut renderer,
        crate::diagnostics::Options::from_environment()?,
        20,
    )?;
    if diagnostics.startup_profile(startup_started.elapsed()) {
        return Ok(());
    }
    let interrupt = Arc::new(AtomicBool::new(false));
    let registration = signal_hook::flag::register(signal_hook::consts::SIGINT, interrupt.clone())?;
    let board_input = if pc {
        None
    } else {
        Some(crate::input::BoardInput::start(config.scale)?)
    };
    let state = crate::network::start()?;
    let mut mouse = Mouse::default();
    let stdin = std::io::stdin();
    let mut reader = stdin.lock();
    let mut spinner = Spinner::default();
    let mut viewer = match kind {
        Kind::Spinner => None,
        Kind::Text => Some(TextWindow::new(
            text.as_deref().unwrap_or(&default_text),
            config,
            &renderer,
        )),
    };
    let result = (|| {
        while !renderer.should_close() && !interrupt.load(Ordering::Relaxed) {
            let mut events: Vec<MouseEvent> = match &board_input {
                Some(input) => input.drain()?,
                None => Vec::new(),
            };
            if pc {
                for slot in 0..2 {
                    let (position, down) = renderer.sample(i32::from(slot));
                    if let Some(event) = mouse.sample(slot, position, down, renderer.time()) {
                        events.push(event);
                    }
                }
            }
            let label = state
                .lock()
                .map_err(|_| Error::Contract("IP monitor lock poisoned"))?
                .label(6999);
            let dt = renderer.frame_time();
            let wheel = renderer.wheel();
            let frame_started = std::time::Instant::now();
            renderer.begin();
            let clicked = match &mut viewer {
                None => {
                    for text in crate::input::read_stdin(&mut reader)? {
                        spinner.set_text(&text, config, &renderer)?;
                    }
                    spinner.render(config, &label, dt, &mut renderer)?;
                    false
                }
                Some(viewer) => viewer.render(config, &label, &events, wheel, &mut renderer)?,
            };
            if diagnostics.finish(&mut renderer, &events, frame_started)? {
                break;
            }
            if clicked {
                if !pc {
                    let control = openpilot_hardware_control::HardwareControl::board(&device);
                    let launcher =
                        std::env::current_exe()?.with_file_name("openpilot-process-child");
                    let mut platform = openpilot_hardware_control::LinuxPlatform::new(
                        std::path::Path::new("/"),
                        openpilot_hardware_control::ProcessCommands { launcher },
                    );
                    control
                        .reboot(&mut platform)
                        .map_err(|_| Error::Contract("board reboot failed"))?;
                }
                break;
            }
        }
        Ok(())
    })();
    drop(board_input);
    signal_hook::low_level::unregister(registration);
    result
}
