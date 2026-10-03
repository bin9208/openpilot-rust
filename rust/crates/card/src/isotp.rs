//! Source-compatible diagnostic CAN transport from `opendbc/car/uds.py`.
//! The adapter owns blocking, clock and outgoing CAN authority. This module never opens a bus.
mod message;
mod receive;
mod transport;

pub use message::IsoTpMessage;
pub use transport::{rx_address, CanClient, CanIo, Error};
