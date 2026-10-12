use std::path::PathBuf;
mod entry;
pub mod input;
pub mod output;
mod placement;
mod poll;
pub mod profile;
mod resources;
pub mod route;
mod run;
mod stop;
mod timing;
pub use entry::command_line;
pub use run::{run, RunReport};
pub use timing::Timing;

#[derive(Clone, Debug)]
pub struct HelperConfig {
    pub program: PathBuf,
    pub launcher: PathBuf,
    pub cwd: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cluster output stopped before renderer startup")]
    Stopped,
    #[error("cluster interrupted")]
    Interrupted,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Cluster(#[from] crate::Error),
    #[error(transparent)]
    Live(#[from] crate::live::Error),
    #[error(transparent)]
    Renderer(#[from] crate::renderer::Error),
    #[error(transparent)]
    Route(#[from] crate::route_replay::Error),
    #[error(transparent)]
    Navi(#[from] crate::navi_simulator::Error),
    #[error(transparent)]
    Gamepad(#[from] crate::simulator::gamepad::GamepadError),
    #[error(transparent)]
    Tools(#[from] crate::replay_tools::Error),
    #[error("{0}")]
    Invalid(String),
    #[error("H264 pipeline initialization failed: {0}")]
    H264Initialization(#[source] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
