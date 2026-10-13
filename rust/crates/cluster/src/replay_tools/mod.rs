mod config;
mod dialog;
mod folder;
mod font_inventory;
mod helper;
mod window;
mod wire;
pub use config::{GuiConfig, WindowConfig};
pub use dialog::choose_log_file;
pub use folder::{load_last_folder, save_last_folder};
pub use helper::run_helper;
pub use window::RouteReplayToolsWindow;
mod events;
mod fonts;
mod raster;
mod render;
mod scale;
mod state;
pub use events::{InputState, UiAction};
pub use fonts::{FontConfig, Fonts};
pub use raster::Canvas;
pub use render::{draw, render};
pub use scale::{smooth_scale, ScaleBackend};
pub use state::ToolsState;
mod action;
mod layout;
pub mod queue;
pub use action::{fold_commands, RouteReplayToolAction, ToolCommand};
pub use layout::{format_time, option_rects, seek_value, Layout, Rect, OPTION_LABELS};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Supervision(#[from] openpilot_process_supervision::Error),
    #[error(transparent)]
    Gui(#[from] openpilot_sdl2::gui::Error),
    #[error("replay tools: {0}")]
    Contract(&'static str),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
