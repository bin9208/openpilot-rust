pub mod arguments;
pub mod entry;
pub mod forwarding;
pub mod heap;
pub mod http_socket;
pub mod image;
pub mod ipc;
pub mod logging;
pub mod mailbox;
pub mod methods;
pub mod net;
pub mod policy;
pub mod proxy;
pub mod queue;
pub mod rpc;
pub mod runtime;
pub mod snapshot;
pub mod state;
pub mod supervisor;
pub mod upload_http;
pub mod uploads;
pub mod websocket;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    SourceJson(#[from] openpilot_logmessaged::JsonError),
    #[error(transparent)]
    Version(#[from] openpilot_runtime_version::Error),
    #[error(transparent)]
    Format(#[from] std::fmt::Error),
    #[error(transparent)]
    Clock(#[from] std::time::SystemTimeError),
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error(transparent)]
    Hardware(#[from] openpilot_hardware_info::Error),
    #[error(transparent)]
    TypedParams(#[from] openpilot_params_typed::Error),
    #[error(transparent)]
    Cereal(#[from] capnp::Error),
    #[error(transparent)]
    Schema(#[from] capnp::NotInSchema),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
    #[error(transparent)]
    Ipc(#[from] openpilot_msgq::Error),
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[error(transparent)]
    State(#[from] openpilot_messaging::state::Error),
    #[error(transparent)]
    Registration(#[from] openpilot_registration::Error),
    #[error(transparent)]
    Token(#[from] openpilot_uploader::TransferError),
    #[error(transparent)]
    WebSocket(#[from] tungstenite::Error),
    #[error(transparent)]
    Http(#[from] ureq::Error),
    #[error(transparent)]
    Request(#[from] ureq::http::Error),
    #[error(transparent)]
    Url(#[from] url::ParseError),
    #[error(transparent)]
    Jpeg(#[from] openpilot_jpeg::Error),
    #[error(transparent)]
    Process(#[from] openpilot_process_supervision::Error),
    #[error(transparent)]
    Managed(#[from] openpilot_managed_entry::EntryError),
    #[error(transparent)]
    Crash(#[from] openpilot_crash_reporting::Error),
    #[error("operation stopped")]
    Stopped,
    #[error("IPC receive timed out")]
    Timeout,
    #[error("{0}")]
    Contract(&'static str),
}
