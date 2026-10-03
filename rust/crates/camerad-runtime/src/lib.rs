#![forbid(unsafe_code)]

#[cfg(feature = "native-skip-miri")]
macro_rules! camera_log {
    ($level:ident, $($argument:tt)*) => {
        crate::diagnostics::emit(openpilot_logging::log_site!(), openpilot_logging::record::Level::$level, format!($($argument)*))
    };
}

#[cfg(feature = "native-skip-miri")]
mod diagnostics;
#[cfg(feature = "native-skip-miri")]
pub use diagnostics::{close as close_logging, initialize as initialize_logging};
#[cfg(feature = "native-skip-miri")]
mod runtime;
#[cfg(feature = "native-skip-miri")]
pub use runtime::{run, RuntimeCamera, RuntimeError, RuntimeOptions};

mod frame_state;
pub use frame_state::{FrameState, FrameStateError};

#[cfg(feature = "native-skip-miri")]
mod sensor;
#[cfg(feature = "native-skip-miri")]
pub use sensor::{SensorError, SensorPort};

#[cfg(feature = "native-skip-miri")]
mod isp_memory;
#[cfg(feature = "native-skip-miri")]
mod isp_port;
#[cfg(feature = "native-skip-miri")]
pub use isp_port::{IspConfig, IspPort, IspPortError, OutputMode};

#[cfg(feature = "native-skip-miri")]
mod camera;
#[cfg(feature = "native-skip-miri")]
mod images;
#[cfg(feature = "native-skip-miri")]
mod phy;
#[cfg(feature = "native-skip-miri")]
pub use camera::{CameraConfig, CameraError, CameraPort, FrameClock};
#[cfg(feature = "native-skip-miri")]
mod clock;
#[cfg(feature = "native-skip-miri")]
pub use clock::SystemClock;
