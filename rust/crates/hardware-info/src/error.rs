#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error(transparent)]
    Json(#[from] openpilot_logmessaged::JsonError),
    #[error(transparent)]
    Format(#[from] std::fmt::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error("missing key: {0}")]
    Key(String),
    #[error("attribute operation: {0}")]
    Attribute(&'static str),
    #[error("unsupported value type: {0}")]
    Type(&'static str),
    #[error("invalid value: {0}")]
    Value(&'static str),
    #[error("numeric conversion overflow")]
    Overflow,
    #[error("zero divisor")]
    ZeroDivision,
    #[error("socket operation timed out")]
    Timeout,
    #[error("missing sequence item")]
    Index,
    #[error("device model cache lock poisoned")]
    CachePoisoned,
    #[error("cannot determine home directory")]
    Home,
    #[error("external command exited with {0}")]
    Command(std::process::ExitStatus),
}
impl Error {
    /// Source exception category, useful to callers retaining original catches.
    pub fn category(&self) -> &'static str {
        match self {
            Self::Io(error) => match error.kind() {
                std::io::ErrorKind::NotFound => "FileNotFoundError",
                std::io::ErrorKind::PermissionDenied => "PermissionError",
                std::io::ErrorKind::IsADirectory => "IsADirectoryError",
                std::io::ErrorKind::NotADirectory => "NotADirectoryError",
                std::io::ErrorKind::TimedOut => "TimeoutError",
                std::io::ErrorKind::WouldBlock => "BlockingIOError",
                std::io::ErrorKind::ConnectionRefused => "ConnectionRefusedError",
                _ => "OSError",
            },
            Self::Utf8(_) => "UnicodeDecodeError",
            Self::Json(openpilot_logmessaged::JsonError::Syntax { .. }) => "JSONDecodeError",
            Self::Json(
                openpilot_logmessaged::JsonError::IntegerLimit
                | openpilot_logmessaged::JsonError::Message,
            )
            | Self::Value(_) => "ValueError",
            Self::Key(_) => "KeyError",
            Self::Attribute(_) => "AttributeError",
            Self::Type(_) => "TypeError",
            Self::Timeout => "TimeoutError",
            Self::Overflow => "OverflowError",
            Self::ZeroDivision => "ZeroDivisionError",
            Self::Index => "IndexError",
            Self::Command(_) => "CalledProcessError",
            Self::Params(openpilot_params::Error::Io(error)) => match error.kind() {
                std::io::ErrorKind::PermissionDenied => "PermissionError",
                _ => "OSError",
            },
            Self::Params(
                openpilot_params::Error::InvalidPrefix | openpilot_params::Error::UnknownKey(_),
            ) => "ValueError",
            Self::Format(_) | Self::CachePoisoned | Self::Home => "RuntimeError",
        }
    }
}
