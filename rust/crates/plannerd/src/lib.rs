//! Project-owned planning policies derived from the original planning daemon.

pub mod car_state;
pub mod car_state_decode;
pub mod carrot;
pub mod coasting;
#[cfg(feature = "native-skip-miri")]
pub mod config;
pub mod driving_mode;
pub mod fast_radar;
pub mod following;
pub mod gap_recovery;
pub mod lane_change_gap;
pub mod lane_departure;
pub mod lane_planner;
#[cfg(feature = "native-skip-miri")]
pub mod lateral_mpc;
#[cfg(feature = "native-skip-miri")]
pub mod lateral_planner;
pub mod lead;
pub mod lead_dynamics;
pub mod lead_obstacles;
pub mod lead_response;
#[cfg(feature = "native-skip-miri")]
pub mod longitudinal_mpc;
#[cfg(feature = "native-skip-miri")]
pub mod longitudinal_planner;
pub mod model;
pub mod model_decode;
#[cfg(feature = "native-skip-miri")]
pub mod native_parameters;
pub mod number;
pub mod parameters;
pub mod path_geometry;
#[cfg(feature = "native-skip-miri")]
#[expect(
    unsafe_code,
    reason = "planner scheduling preserves the native Linux FIFO ABI"
)]
pub mod platform;
pub mod preview;
#[cfg(feature = "native-skip-miri")]
pub mod publication;
pub mod radar;
pub mod radar_decode;
#[cfg(feature = "native-skip-miri")]
pub mod runtime;
pub mod stopping_lead;
pub mod traffic_stop;
pub mod turn_accel;
pub mod types;
pub mod window;

#[cfg_attr(
    feature = "native-skip-miri",
    expect(unsafe_code, reason = "isolated ownership of the pinned acados C ABI")
)]
pub mod solver;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("plannerd: {0}")]
    Contract(&'static str),
    #[error("acados {operation} returned {status}")]
    Solver {
        operation: &'static str,
        status: i32,
    },
    #[cfg(feature = "native-skip-miri")]
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[cfg(feature = "native-skip-miri")]
    #[error(transparent)]
    State(#[from] openpilot_messaging::state::Error),
    #[cfg(feature = "native-skip-miri")]
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
    #[error(transparent)]
    Enum(#[from] capnp::NotInSchema),
    #[cfg(feature = "native-skip-miri")]
    #[error(transparent)]
    Parameters(#[from] openpilot_params::Error),
    #[cfg(feature = "native-skip-miri")]
    #[error(transparent)]
    Integer(#[from] openpilot_beepd::IntegerError),
    #[cfg(feature = "native-skip-miri")]
    #[error(transparent)]
    Float(#[from] openpilot_calibrationd::Error),
    #[error(transparent)]
    Cereal(#[from] capnp::Error),
    #[error(transparent)]
    Policy(#[from] openpilot_control_policy::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[cfg(feature = "native-skip-miri")]
    #[error(transparent)]
    Library(#[from] libloading::Error),
}
