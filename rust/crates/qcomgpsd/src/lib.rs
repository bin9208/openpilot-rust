//! Native project-owned qcomgpsd policy; Linux UART and modem firmware stay external.
pub mod at;
pub mod decode;
mod dr;
pub mod framing;
mod measurement;
mod poly;
pub mod position;
pub mod reader;
pub mod runtime;
pub mod serial;
mod status;
mod wire;
pub mod reports {
    include!(concat!(env!("OUT_DIR"), "/reports.rs"));
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Serial(#[from] serialport::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Syscall(#[from] rustix::io::Errno),
    #[error(transparent)]
    Capnp(#[from] capnp::Error),
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error(transparent)]
    Hardware(#[from] openpilot_hardware_control::Error),
    #[error(transparent)]
    Clock(#[from] openpilot_timed::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Protocol(&'static str),
    #[error("NMEA modem reboot requested")]
    NmeaReboot,
    #[error("qcomgpsd stop requested")]
    Stopped,
    #[error("serial AT command timed out: {0}")]
    AtTimeout(String),
    #[error("command {command} exited {status}")]
    Command { command: String, status: i32 },
}

pub mod assistance;
pub mod config;
pub mod daemon;
pub mod nmea;
pub mod setup;

pub mod nmea_io;
