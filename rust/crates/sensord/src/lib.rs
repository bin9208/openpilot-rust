//! Port of system/sensord and the called common/i2c.py and gpiochip boundaries.
#[cfg(feature = "native")]
#[allow(unsafe_code)]
mod bridge;
pub mod clock;
#[cfg(feature = "native")]
pub mod linux;
pub mod loops;
#[cfg(feature = "native")]
pub mod runtime;
pub mod self_test;
pub mod sensor;
pub mod wire;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sensor data not ready")]
    DataNotReady,
    #[error("{0}")]
    Sensor(String),
    #[error("{0}")]
    Contract(&'static str),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Capnp(#[from] capnp::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[cfg(feature = "native")]
    #[error(transparent)]
    Kernel(#[from] cxx::Exception),
    #[cfg(feature = "native")]
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[cfg(feature = "native")]
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[cfg(feature = "native")]
    #[error(transparent)]
    Hardware(#[from] openpilot_hardware_control::Error),
}
pub trait Bus {
    fn read(&mut self, register: u8, length: usize) -> Result<Vec<u8>, Error>;
    fn write(&mut self, register: u8, value: u8) -> Result<(), Error>;
}
pub trait Clock {
    fn monotonic(&mut self) -> f64;
    fn monotonic_ns(&mut self) -> i128;
    fn realtime_ns(&mut self) -> i128;
    fn sleep(&mut self, seconds: f64);
}

#[cfg(test)]
mod tests;
