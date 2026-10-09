//! Shared compact-state encoding from `selfdrive/carrot/realtime/compact_state.py`.
mod compact;
mod compact_fields;
mod compact_schema;
pub mod value;

pub use compact::{batch, encode, encode_reader, services};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Source(String),
}
