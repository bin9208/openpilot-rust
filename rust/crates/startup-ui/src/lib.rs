//! Native startup surfaces and owning process wrappers; source MIT provenance retained.
pub mod config;
mod digits;
pub mod draw;
pub mod geometry;
pub mod network;
pub mod scroll;
pub mod spinner;
pub mod text;
pub mod text_window;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("startup UI: {0}")]
    Contract(&'static str),
    #[error("invalid spinner integer")]
    Integer,
    #[cfg(feature = "native")]
    #[error(transparent)]
    Native(#[from] cxx::Exception),
    #[error(transparent)]
    Process(#[from] openpilot_process_supervision::Error),
}

#[cfg(feature = "native")]
#[expect(
    unsafe_code,
    reason = "audited CXX boundary to the existing external raylib"
)]
mod bridge;
#[cfg(feature = "native")]
pub mod renderer;

#[cfg(feature = "native")]
pub mod app;
pub mod input;

pub mod children;
pub mod native_children;

mod number;

#[cfg(feature = "native")]
pub mod renderer_controls;

#[cfg(feature = "native")]
pub mod diagnostics;
#[cfg(feature = "native")]
pub mod egl;
#[cfg(feature = "native")]
pub mod logging;
#[cfg(feature = "native")]
mod renderer_polygon;
