//! Rust procLog serialization and standalone collector support.
pub mod wire;

pub const PROC_LOG_QUEUE_SIZE: usize = 10 * 1024 * 1024;
