//! Rust-owned Panda SPI transaction policy. Linux transport composition is separate.
pub mod device;
mod diagnostics;
mod priority;
pub mod protocol;
pub use diagnostics::ErrorEvent;
pub mod linux_io;
