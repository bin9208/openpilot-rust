//! Bounded wire peers for the recovery controller family.
//!
//! These peers deliberately speak the public HTTP and WebSocket protocol. They
//! do not call the recovery implementation or supply expected responses.

mod actions_control;
mod actions_effects;
mod cwp_control;
mod http;
mod http_control;
pub mod original_fixture;
mod process;
mod server;
mod source;
mod support_control;
mod support_shutdown;
mod terminal_control;
mod terminal_lifecycle;
mod upload_control;
mod upload_wire;
mod websocket;

pub use actions_control::{
    admit_actions_corpus, run_actions_frozen, ActionsCorpusConfig, ActionsFrozenConfig,
};
pub use cwp_control::{run_cwp_frozen, run_cwp_pair, CwpFrozenConfig, CwpPairConfig};
pub use http::{stable_headers, HttpPeer, HttpResponse};
pub use http_control::{run_http_frozen, run_http_pair, HttpFrozenConfig, HttpPairConfig};
pub use process::OwnedServer;
pub use server::{CapturedRequest, Reply, ScriptedHttpPeer};
pub use source::Pin;
pub use support_control::{
    run_support_frozen, run_support_pair, SupportFrozenConfig, SupportPairConfig,
};
pub use support_shutdown::{
    run_support_shutdown, run_support_shutdown_frozen, SupportShutdownConfig,
};
pub use terminal_control::{
    run_terminal_frozen, run_terminal_pair, TerminalFrozenConfig, TerminalPairConfig,
};
pub use terminal_lifecycle::{
    run_terminal_lifecycle, run_terminal_lifecycle_frozen, TerminalLifecycleConfig,
};
pub use upload_control::{
    admit_upload_corpus, run_upload_frozen, UploadCorpusConfig, UploadFrozenConfig,
};
pub use websocket::{encode_frame, Frame, WebSocketPeer};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
