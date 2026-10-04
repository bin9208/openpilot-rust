pub mod base;
pub mod batch;
pub mod clustering;
pub mod data;
pub mod databases;
pub mod decoder;
pub mod integer_set;
pub mod lead_filter;
pub mod native;
#[allow(unsafe_code)]
pub mod numerics;
#[allow(unsafe_code)]
mod numerics_dot;
#[expect(
    unsafe_code,
    reason = "SVD buffer ABI has injected Miri fixtures; actual pinned foreign library remains external"
)]
mod numerics_weights;
pub mod point;
pub mod reader;
pub mod runtime;
pub mod scalar;
pub mod settings;
pub mod track;
pub mod wire;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("radarcan interrupted by signal {0}")]
    Signal(i32),
    #[error("radar sample period must be finite and positive")]
    InvalidPeriod,
    #[error("float division by zero")]
    DivisionByZero,
    #[error("SVD did not converge")]
    SvdDidNotConverge,
    #[error("cannot convert float infinity to integer")]
    InfiniteInteger,
    #[error("cannot convert float NaN to integer")]
    NanInteger,
    #[error("maxlen must be non-negative")]
    NegativeHistory,
    #[error("deque index out of range")]
    EmptyHistory,
    #[error("Python int too large to convert to C ssize_t")]
    IntegerOverflow,
    #[error("(34, 'Numerical result out of range')")]
    PowerOverflow,
    #[error("{0}")]
    Contract(&'static str),
    #[error("radar DBC metadata is absent for {0}")]
    MissingRadarDbc(String),
    #[error("radar interface remains unported: {0}")]
    UnportedRadar(String),
    #[error("Unsupported radar: {}", .0.as_deref().unwrap_or("None"))]
    UnsupportedRadar(Option<String>),
    #[error("inherited fatal Params.get_int({key}) boundary: {error}")]
    ParameterInteger {
        key: &'static str,
        error: openpilot_beepd::IntegerError,
    },
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Vehicle(#[from] openpilot_card::vehicle_params::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Library(#[from] libloading::Error),
    #[error(transparent)]
    Can(#[from] openpilot_can::Error),
    #[error(transparent)]
    Format(#[from] std::fmt::Error),
    #[error(transparent)]
    Wire(#[from] wire::Error),
    #[error(transparent)]
    Transport(#[from] openpilot_msgq::Error),
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error("invalid LOGPRINT logging level: {0}")]
    LoggingLevel(String),
    #[error("required radar DBC asset {}: {error}", path.display())]
    DbcAsset {
        path: std::path::PathBuf,
        error: std::io::Error,
    },
}
