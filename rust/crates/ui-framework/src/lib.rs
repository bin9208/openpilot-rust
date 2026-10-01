//! Rust-owned shared UI behavior ported from system/ui/{lib,widgets} (MIT).
pub use openpilot_startup_ui::{draw, geometry, text, Error};
pub mod layouts;
pub mod widget;

pub mod animation;
#[cfg(feature = "native")]
pub mod application;
pub mod assets;
pub mod button;
pub mod callback;
#[cfg(feature = "native")]
pub mod canvas;
pub mod dialog;
pub mod emoji;
pub mod html;
pub mod icon;
pub mod inputbox;
pub mod keyboard;
pub mod keys;
pub mod label;
pub mod list;
pub mod mici_keyboard;
pub mod multilang;
pub mod navigation;
pub mod network;
pub mod polygon;
pub mod scroll;
pub mod scroller;
pub mod scroller_tici;
pub mod slider;
pub mod stack;
pub mod styled_text;
pub mod text_layout;
pub mod toggle;
pub mod unified_label;
#[cfg(feature = "native")]
pub use openpilot_startup_ui::egl;
