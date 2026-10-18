//! Final source S3 transport; Wi-Fi credentials are never protocol payloads.
//! Ported from e8e152dbcfd9574a1791ed12026d1880e87689b3:
//! openpilot/selfdrive/carrot/cluster/cluster_wifi_protocol.py (MIT).
mod config;
mod discovery;
mod protocol;
mod queue;
mod tcp;
pub use config::{config_path, decode_hex, save_config, Registration};
pub use discovery::{discover, discover_with, Datagram};
pub use protocol::{mac, Connection, Socket};
pub use queue::{Channel, Queue, TransactionError};
use std::io;
pub use tcp::{connect, TcpSocket};

pub const PORT: u16 = 3333;
pub const MAX_PAYLOAD: usize = 1024 * 1024;
pub const MAGIC: u32 = 0x5333_5531;
pub const ACK: u32 = 0x5333_4131;
pub const HEADER_BYTES: usize = 24;
pub const DISCOVER: &[u8; 4] = b"S3D1";
pub const FOUND: &[u8; 4] = b"S3R1";
pub const HELLO: &[u8; 4] = b"S3H1";
pub const DEFAULT_CONFIG: &str = "/data/carrot/cluster_wifi.json";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid S3 registration file; register the display again")]
    InvalidRegistration,
    #[error("'{0}'")]
    MissingConfigField(&'static str),
    #[error("Registered S3 display not found on this Wi-Fi")]
    NotFound,
    #[error("S3 display disconnected")]
    Disconnected,
    #[error("S3 display is closed")]
    DisplayClosed,
    #[error("S3 acknowledgement sequence mismatch")]
    AckSequence,
    #[error("S3 device identity mismatch")]
    Identity,
    #[error("S3 authentication failed")]
    Authentication,
    #[error("S3 payload must be 512..1048576 bytes")]
    Payload,
    #[error("'I' format requires 0 <= number <= 4294967295")]
    Sequence,
    #[error("Invalid S3 acknowledgement")]
    Acknowledgement,
    #[error("Invalid S3 acknowledgement signature")]
    Signature,
    #[error("S3 USB transaction failed (status {0})")]
    Usb(u32),
    #[error("{0}")]
    Io(#[from] io::Error),
}

mod capabilities;
pub use capabilities::{
    capabilities, capabilities_with, capability_reply, capability_request, select_codec,
    select_codec_with, Codec,
};
