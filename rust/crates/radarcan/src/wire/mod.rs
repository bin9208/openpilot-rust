mod input;
mod output;
pub use input::{can, config, ego};
pub use output::encode;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("input length must be a multiple of eight bytes")]
    ByteLength,
    #[error("radar subscription received a different cereal event: expected {0}")]
    Event(&'static str),
    #[error(transparent)]
    Schema(#[from] capnp::Error),
    #[error(transparent)]
    Enum(#[from] capnp::NotInSchema),
    #[error(transparent)]
    Text(#[from] std::str::Utf8Error),
    #[error(transparent)]
    Count(#[from] std::num::TryFromIntError),
}

fn read(bytes: &[u8]) -> Result<capnp::message::Reader<capnp::serialize::OwnedSegments>, Error> {
    if !bytes.len().is_multiple_of(8) {
        return Err(Error::ByteLength);
    }
    let mut options = capnp::message::ReaderOptions::new();
    options.traversal_limit_in_words = None;
    Ok(capnp::serialize::read_message(
        std::io::Cursor::new(bytes),
        options,
    )?)
}
