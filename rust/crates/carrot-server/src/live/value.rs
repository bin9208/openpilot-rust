use crate::Error;
use capnp::{dynamic_value::Reader, message::ReaderOptions, serialize::OwnedSegments};
use num_traits::ToPrimitive;
use openpilot_cereal::log_capnp::event;

pub(super) fn message(bytes: &[u8]) -> Result<capnp::message::Reader<OwnedSegments>, Error> {
    let mut options = ReaderOptions::new();
    options.traversal_limit_in_words(None);
    capnp::serialize::read_message(&mut std::io::Cursor::new(bytes), options)
        .map_err(|error| Error::Source(error.to_string()))
}
pub(super) fn service<'a>(
    message: &'a capnp::message::Reader<OwnedSegments>,
    name: &str,
) -> Result<Reader<'a>, Error> {
    let root: event::Reader<'_> = message
        .get_root()
        .map_err(|error| Error::Source(error.to_string()))?;
    let Reader::Struct(root) = root.into() else {
        return Err(Error::Source("Event must be a struct".into()));
    };
    root.get_named(name)
        .map_err(|error| Error::Source(error.to_string()))
}
pub(super) fn field<'a>(value: Reader<'a>, name: &str) -> Reader<'a> {
    match value {
        Reader::Struct(value) => value.get_named(name).unwrap_or(Reader::Void),
        Reader::Void
        | Reader::Bool(_)
        | Reader::Int8(_)
        | Reader::Int16(_)
        | Reader::Int32(_)
        | Reader::Int64(_)
        | Reader::UInt8(_)
        | Reader::UInt16(_)
        | Reader::UInt32(_)
        | Reader::UInt64(_)
        | Reader::Float32(_)
        | Reader::Float64(_)
        | Reader::Enum(_)
        | Reader::Text(_)
        | Reader::Data(_)
        | Reader::List(_)
        | Reader::AnyPointer(_)
        | Reader::Capability(_) => Reader::Void,
    }
}
pub(super) fn path<'a>(mut value: Reader<'a>, names: &[&str]) -> Reader<'a> {
    for name in names {
        value = field(value, name);
    }
    value
}
pub(super) fn integer(value: Reader<'_>) -> i128 {
    match value {
        Reader::Bool(value) => i128::from(value),
        Reader::Int8(value) => i128::from(value),
        Reader::Int16(value) => i128::from(value),
        Reader::Int32(value) => i128::from(value),
        Reader::Int64(value) => i128::from(value),
        Reader::UInt8(value) => i128::from(value),
        Reader::UInt16(value) => i128::from(value),
        Reader::UInt32(value) => i128::from(value),
        Reader::UInt64(value) => i128::from(value),
        Reader::Float32(value) => value.to_i128().unwrap_or(0),
        Reader::Float64(value) => value.to_i128().unwrap_or(0),
        Reader::Enum(value) => i128::from(value.get_value()),
        Reader::Void
        | Reader::Text(_)
        | Reader::Data(_)
        | Reader::Struct(_)
        | Reader::List(_)
        | Reader::AnyPointer(_)
        | Reader::Capability(_) => 0,
    }
}
pub(super) fn number(value: Reader<'_>, default: f64) -> f64 {
    match value {
        Reader::Float32(value) => f64::from(value),
        Reader::Float64(value) => value,
        Reader::Void
        | Reader::Text(_)
        | Reader::Data(_)
        | Reader::Struct(_)
        | Reader::List(_)
        | Reader::AnyPointer(_)
        | Reader::Capability(_) => default,
        Reader::Bool(_)
        | Reader::Int8(_)
        | Reader::Int16(_)
        | Reader::Int32(_)
        | Reader::Int64(_)
        | Reader::UInt8(_)
        | Reader::UInt16(_)
        | Reader::UInt32(_)
        | Reader::UInt64(_)
        | Reader::Enum(_) => integer(value).to_f64().unwrap_or(default),
    }
}
pub(super) fn truth(value: Reader<'_>) -> bool {
    match value {
        Reader::Void => false,
        Reader::Bool(value) => value,
        Reader::Text(value) => !value.as_bytes().is_empty(),
        Reader::Data(value) => !value.is_empty(),
        Reader::List(value) => !value.is_empty(),
        Reader::Struct(_) | Reader::AnyPointer(_) | Reader::Capability(_) => true,
        Reader::Int8(_)
        | Reader::Int16(_)
        | Reader::Int32(_)
        | Reader::Int64(_)
        | Reader::UInt8(_)
        | Reader::UInt16(_)
        | Reader::UInt32(_)
        | Reader::UInt64(_)
        | Reader::Float32(_)
        | Reader::Float64(_)
        | Reader::Enum(_) => number(value, 0.) != 0.,
    }
}
pub(super) fn text(value: Reader<'_>) -> String {
    match value {
        Reader::Text(value) => String::from_utf8_lossy(value.as_bytes()).into_owned(),
        Reader::Data(value) => String::from_utf8_lossy(value).into_owned(),
        Reader::Enum(value) => match value.get_enumerant() {
            Ok(Some(value)) => value
                .get_proto()
                .get_name()
                .map(|name| String::from_utf8_lossy(name.as_bytes()).into_owned())
                .unwrap_or_default(),
            Ok(None) | Err(_) => value.get_value().to_string(),
        },
        Reader::Void => String::new(),
        Reader::Bool(value) => if value { "True" } else { "False" }.into(),
        Reader::Int8(_)
        | Reader::Int16(_)
        | Reader::Int32(_)
        | Reader::Int64(_)
        | Reader::UInt8(_)
        | Reader::UInt16(_)
        | Reader::UInt32(_)
        | Reader::UInt64(_) => integer(value).to_string(),
        Reader::Float32(_) | Reader::Float64(_) => number(value, 0.).to_string(),
        Reader::Struct(_) | Reader::List(_) | Reader::AnyPointer(_) | Reader::Capability(_) => {
            String::new()
        }
    }
}
pub(super) fn data(value: Reader<'_>) -> Vec<u8> {
    match value {
        Reader::Data(value) => value.to_vec(),
        Reader::Text(value) => value.as_bytes().to_vec(),
        Reader::Void
        | Reader::Bool(_)
        | Reader::Int8(_)
        | Reader::Int16(_)
        | Reader::Int32(_)
        | Reader::Int64(_)
        | Reader::UInt8(_)
        | Reader::UInt16(_)
        | Reader::UInt32(_)
        | Reader::UInt64(_)
        | Reader::Float32(_)
        | Reader::Float64(_)
        | Reader::Enum(_)
        | Reader::Struct(_)
        | Reader::List(_)
        | Reader::AnyPointer(_)
        | Reader::Capability(_) => Vec::new(),
    }
}
