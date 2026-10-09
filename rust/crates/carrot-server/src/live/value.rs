use crate::Error;
use capnp::{dynamic_value::Reader, serialize::OwnedSegments};
pub(super) use openpilot_carrot_state::value::{data, field, integer, number, text, truth};

pub(super) fn message(bytes: &[u8]) -> Result<capnp::message::Reader<OwnedSegments>, Error> {
    openpilot_carrot_state::value::message(bytes).map_err(|error| Error::Source(error.to_string()))
}

pub(super) fn service<'a>(
    message: &'a capnp::message::Reader<OwnedSegments>,
    name: &str,
) -> Result<Reader<'a>, Error> {
    openpilot_carrot_state::value::service(message, name)
        .map_err(|error| Error::Source(error.to_string()))
}
