#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Cereal(#[from] capnp::Error),
    #[error(transparent)]
    Enum(#[from] capnp::NotInSchema),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
    #[error(transparent)]
    Input(#[from] crate::car_specific::InputError),
    #[error(transparent)]
    Health(#[from] openpilot_messaging::state::Error),
    #[error(transparent)]
    Parameter(#[from] crate::callbacks::Error),
    #[error(transparent)]
    CarPolicy(#[from] crate::car_specific::Error<crate::callbacks::Error>),
    #[error(transparent)]
    MissingAlertText(#[from] crate::alerts::MissingAlertText),
    #[error(transparent)]
    Actuation(#[from] crate::helpers::Error),
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Catalog(#[from] serde_json::Error),
    #[error("event {0:?} has no alert definition")]
    UndefinedEvent(openpilot_cereal::log_capnp::onroad_event::EventName),
    #[error(transparent)]
    Number(#[from] std::num::ParseFloatError),
    #[error(transparent)]
    Json(#[from] openpilot_logmessaged::JsonError),
    #[error(transparent)]
    Format(#[from] std::fmt::Error),
    #[error(transparent)]
    Transport(#[from] openpilot_msgq::Error),
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[error(transparent)]
    Version(#[from] openpilot_runtime_version::Error),
    #[error(transparent)]
    Hardware(#[from] openpilot_hardware_info::Error),
    #[error(transparent)]
    Language(#[from] openpilot_ui_framework::multilang::Error),
    #[error("personality has no source enum name: {0}")]
    Personality(String),
    #[error("source policy division by zero")]
    ZeroDivision,
    #[error("runtime settings lock poisoned")]
    SettingsPoisoned,
    #[error("invalid selfdrived input: {0}")]
    Contract(&'static str),
}
