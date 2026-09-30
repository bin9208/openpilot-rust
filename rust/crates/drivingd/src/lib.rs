pub mod camera;
pub mod parameters;
pub mod publication;
pub mod runtime;
pub mod state;
pub mod wire;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("driving model contract: {0}")]
    Contract(&'static str),
    #[error(transparent)]
    Runtime(#[from] openpilot_model_runtime::Error),
    #[error(transparent)]
    Parse(#[from] openpilot_modeld::parse::ParseError),
    #[error(transparent)]
    Features(#[from] openpilot_modeld::inputs::FeatureCount),
    #[error(transparent)]
    Camera(#[from] openpilot_modeld::calibration::UnknownCamera),
    #[error(transparent)]
    Transport(#[from] openpilot_msgq::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Cereal(#[from] capnp::Error),
    #[error(transparent)]
    Schema(#[from] capnp::NotInSchema),
    #[error(transparent)]
    Desire(#[from] openpilot_desire::types::InvalidModel),
    #[error(transparent)]
    Text(#[from] std::str::Utf8Error),
}
