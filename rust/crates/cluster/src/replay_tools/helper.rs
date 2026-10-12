use super::{
    choose_log_file, draw, load_last_folder,
    queue::Latest,
    save_last_folder,
    wire::{Incoming, Outgoing, Packet, Pump},
    Canvas, Fonts, GuiConfig, InputState, Layout, Result, ScaleBackend, ToolCommand, ToolsState,
    UiAction,
};
use openpilot_sdl2::gui::Context;
use std::{
    os::unix::net::UnixStream,
    path::Path,
    sync::{
        mpsc::{self, SyncSender, TrySendError},
        Arc,
    },
    time::{Duration, Instant},
};
fn command(sender: &SyncSender<ToolCommand>, command: ToolCommand) {
    match sender.try_send(command) {
        Ok(()) | Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {}
    }
}
fn position() -> (i32, i32) {
    let Some(value) = std::env::var_os("SDL_VIDEO_WINDOW_POS") else {
        return (20, 510);
    };
    let value = value.to_string_lossy();
    if value == "center" {
        return (0x2fff_0000, 0x2fff_0000);
    }
    let Some((x, y)) = value.split_once(',') else {
        return (0, 0);
    };
    (x.parse().unwrap_or(0), y.parse().unwrap_or(0))
}
/// # Errors
/// Returns malformed configuration, IPC, saved-folder or native window errors.
pub fn run_helper(socket: &Path, config: &str) -> Result<()> {
    let config: GuiConfig = serde_json::from_str(config)?;
    let stream = UnixStream::connect(socket)?;
    let updates = Arc::new(Latest::default());
    let (sender, receiver) = mpsc::sync_channel(16);
    let mut pump = Pump::spawn(
        stream,
        Outgoing::Commands(receiver),
        Incoming::Latest(Arc::clone(&updates)),
    )?;
    let context = match Context::load(&config.sdl_library, &config.ttf_library) {
        Ok(context) => context,
        Err(error) => {
            eprintln!("replay GUI provider unavailable: {error}");
            command(&sender, ToolCommand::Closed);
            std::thread::sleep(Duration::from_millis(20));
            pump.close()?;
            return Ok(());
        }
    };
    let mut window = context.window("Carrot Cluster Replay Tools", 1100, 520, position(), true)?;
    let fonts = Fonts::load(&context, &config.fonts)?;
    let mut state = ToolsState::default();
    let mut input = InputState::default();
    let mut last_folder = load_last_folder(&config.last_folder_file)?;
    let mut canvas = Canvas::new(1100, 520)?;
    let mut running = true;
    while running && pump.is_alive() {
        let start = Instant::now();
        if let Some(update) = updates.take() {
            match update {
                Packet::State { state: next } => state = *next,
                Packet::Stop => running = false,
                Packet::Command { .. } => {
                    return Err(super::Error::Contract("helper received command"));
                }
            }
        }
        let (width, height) = window.size();
        let layout = Layout::new(width, height)?;
        for event in window.events() {
            if let Some(action) = input.handle(event, &layout, &mut state) {
                match action {
                    UiAction::Command(ToolCommand::Closed) => running = false,
                    UiAction::Command(value) => command(&sender, value),
                    UiAction::ChooseLog => {
                        let current = state
                            .current_log_folder
                            .trim_matches(openpilot_startup_ui::text::whitespace);
                        let initial = if last_folder.is_empty() {
                            current
                        } else {
                            &last_folder
                        };
                        match choose_log_file(&config, initial) {
                            Ok(Some(selected)) => {
                                last_folder = Path::new(&selected)
                                    .parent()
                                    .map_or_else(String::new, |p| p.to_string_lossy().into_owned());
                                save_last_folder(&config.last_folder_file, &last_folder);
                                command(&sender, ToolCommand::OpenPath { value: selected });
                            }
                            Ok(None) => {}
                            Err(error) => eprintln!("replay file dialog: {error}"),
                        }
                    }
                }
            }
        }
        if (canvas.width, canvas.height) != (width, height) {
            canvas = Canvas::new(width, height)?;
        }
        draw(&mut canvas, &state, &fonts, ScaleBackend::default())?;
        window.present(&canvas.rgba, width, height)?;
        let elapsed = start.elapsed();
        if let Some(remaining) = Duration::from_nanos(33_333_333).checked_sub(elapsed) {
            std::thread::sleep(remaining);
        }
    }
    command(&sender, ToolCommand::Closed);
    std::thread::sleep(Duration::from_millis(20));
    pump.close()?;
    Ok(())
}
