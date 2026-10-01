#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error(transparent)]
    Process(#[from] openpilot_process_supervision::Error),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("D-Bus connection: {0}")]
    Bus(#[from] dbus::Error),
    #[error("D-Bus method reply: {0}")]
    Reply(dbus::Error),
    #[error("invalid D-Bus request: {0}")]
    Request(String),
    #[error("invalid NetworkManager property: {0}")]
    Property(&'static str),
    #[error("Wi-Fi manager stopped")]
    Stopped,
    #[error("Wi-Fi command queue is full")]
    QueueFull,
    #[error("Wi-Fi D-Bus transport stopped: {0}")]
    Transport(String),
    #[error("monotonic clock is out of range")]
    ClockRange,
    #[error("Wi-Fi state lock poisoned")]
    Poisoned,
    #[error("Wi-Fi worker panicked")]
    Panicked,
}
