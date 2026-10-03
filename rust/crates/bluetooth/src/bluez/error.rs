#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Bus(#[from] dbus::Error),
    #[error(transparent)]
    Schema(#[from] dbus::arg::TypeMismatchError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Task(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Policy(#[from] super::policy::Error),
    #[error(transparent)]
    Text(#[from] openpilot_runtime_version::Error),
    #[error(transparent)]
    Json(#[from] openpilot_logmessaged::JsonError),
    #[error(transparent)]
    Encoding(#[from] serde_json::Error),
    #[error("[{name}] {body}")]
    Remote { name: String, body: String },
    #[error("")]
    Timeout,
    #[error("Bluetooth adapter unavailable")]
    Adapter,
    #[error("device not found; scan again")]
    Device,
    #[error("pairing already in progress")]
    Pairing,
    #[error("pairing prompt expired")]
    PromptExpired,
    #[error("Bluetooth state lock poisoned")]
    Poisoned,
    #[error("Bluetooth D-Bus connection closed")]
    Closed,
    #[error("invalid BlueZ property: {0}")]
    Property(&'static str),
    #[error("{0}")]
    Request(String),
}
