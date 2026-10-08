pub mod json;
pub mod naver;
pub mod peers;
pub use json::IngressError;

pub const TCP_MAX_FRAME: usize = 262_144;
pub const TCP_MAX_CLIENTS: usize = 4;
pub const TCP_TIMEOUT_SECONDS: u64 = 10;
