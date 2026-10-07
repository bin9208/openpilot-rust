//! Native port of opendbc CAN signal, checksum, packer and parser policies.
pub mod checksum;
pub mod dbc;
pub mod packer;
pub mod parser;
pub mod signal;
pub mod state;
mod volkswagen;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Frame {
    pub address: u32,
    pub data: Vec<u8>,
    pub bus: u8,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Packet {
    pub mono_time: u64,
    pub frames: Vec<Frame>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Diagnostic {
    pub address: u32,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("DBC: {0}")]
    Dbc(String),
    #[error("unknown CAN message {0}")]
    Message(String),
    #[error("duplicate CAN message {0}")]
    Duplicate(u32),
    #[error("unknown CAN signal {0}")]
    Signal(String),
    #[error("invalid CAN numeric value")]
    Numeric,
    #[error("invalid checksum payload")]
    Checksum,
    #[error("inherited Volkswagen MLB checksum failure: xor_checksum() takes 3 positional arguments but 4 were given")]
    InheritedMlbChecksum,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Regex(#[from] regex::Error),
}
