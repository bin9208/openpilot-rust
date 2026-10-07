pub mod joystick;
#[cfg(feature = "native")]
pub mod joystick_input;
pub mod joystickd;
#[cfg(feature = "native")]
pub mod joystickd_runtime;
pub mod joystickd_wire;
pub mod longitudinal_maneuvers;
#[cfg(feature = "native")]
pub mod longitudinal_maneuvers_runtime;
pub mod longitudinal_maneuvers_wire;
pub mod maneuver;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Policy(#[from] openpilot_control_policy::Error),
    #[error("control tool contract: {0}")]
    Contract(&'static str),
    #[error(transparent)]
    Cereal(#[from] capnp::Error),
    #[error(transparent)]
    MessageState(#[from] openpilot_messaging::state::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[cfg(feature = "native")]
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[cfg(feature = "native")]
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
}
