use crate::{Error, Value};
use capnp::{dynamic_value::Reader, message::ReaderOptions};
use openpilot_cereal::log_capnp::event;

pub(super) fn state(bytes: &[u8]) -> Result<Value, Error> {
    let mut options = ReaderOptions::new();
    options.traversal_limit_in_words(None);
    let message = capnp::serialize::read_message(&mut std::io::Cursor::new(bytes), options)
        .map_err(|error| Error::Source(error.to_string()))?;
    let root: event::Reader<'_> = message
        .get_root()
        .map_err(|error| Error::Source(error.to_string()))?;
    let event::Which::CarrotNavi(data) = root
        .which()
        .map_err(|error| Error::Source(error.to_string()))?
    else {
        return Err(Error::Source("expected CarrotNavi event".into()));
    };
    let data = data.map_err(|error| Error::Source(error.to_string()))?;
    value(data.into()).map_err(|error| Error::Source(error.to_string()))
}
fn value(reader: Reader<'_>) -> Result<Value, capnp::Error> {
    Ok(match reader {
        Reader::Void => Value::Null,
        Reader::Bool(value) => Value::Bool(value),
        Reader::Int8(value) => Value::integer(value),
        Reader::Int16(value) => Value::integer(value),
        Reader::Int32(value) => Value::integer(value),
        Reader::Int64(value) => Value::integer(value),
        Reader::UInt8(value) => Value::integer(value),
        Reader::UInt16(value) => Value::integer(value),
        Reader::UInt32(value) => Value::integer(value),
        Reader::UInt64(value) => Value::integer(value),
        Reader::Float32(value) => Value::Float(f64::from(value)),
        Reader::Float64(value) => Value::Float(value),
        Reader::Text(value) => Value::text(value.to_str()?),
        Reader::Data(value) => Value::text(&String::from_utf8_lossy(value)),
        Reader::Enum(value) => match value.get_enumerant()? {
            Some(item) => Value::text(item.get_proto().get_name()?.to_str()?),
            None => Value::integer(value.get_value()),
        },
        Reader::List(items) => Value::Array(
            items
                .iter()
                .map(|item| value(item?))
                .collect::<Result<_, _>>()?,
        ),
        Reader::Struct(data) => {
            let mut fields = Vec::new();
            for field in data.get_schema().get_fields()? {
                if data.has(field)? {
                    let name = field.get_proto().get_name()?.to_str()?;
                    fields.push((
                        name.chars().map(u32::from).collect(),
                        value(data.get(field)?)?,
                    ));
                }
            }
            Value::Object(fields)
        }
        Reader::AnyPointer(_) | Reader::Capability(_) => Value::Null,
    })
}
