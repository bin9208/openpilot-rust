//! Rust-owned shared UI behavior ported from system/ui/{lib,widgets} (MIT).
pub use openpilot_startup_ui::{draw, geometry, text, Error};
pub mod layouts;
pub mod widget;

pub mod scroll;
