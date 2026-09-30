#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Schema(#[from] capnp::Error),
    #[error(transparent)]
    SchemaEnum(#[from] capnp::NotInSchema),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Media(#[from] ffmpeg_next::Error),
    #[error(transparent)]
    Messaging(#[from] openpilot_msgq::Error),
    #[error("logger boundary: {0}")]
    Invalid(&'static str),
    #[error("encoded video closed before its header; retained lock {}", .lock.display())]
    IncompleteVideo { lock: std::path::PathBuf },
    #[error("random route identifier: {0}")]
    Random(#[from] getrandom::Error),
    #[error(transparent)]
    Integer(#[from] std::num::TryFromIntError),
}
