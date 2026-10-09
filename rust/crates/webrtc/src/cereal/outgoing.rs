use super::{read, Error};
use capnp::dynamic_value::Reader;
use openpilot_cereal::log_capnp::event;
use openpilot_logging::{Fields, Value};

fn value(reader: Reader<'_>, in_dict: bool) -> Result<Value, Error> {
    Ok(match reader {
        Reader::Void => Value::Null,
        Reader::Bool(v) => Value::Bool(v),
        Reader::Int8(v) => Value::Integer(i128::from(v)),
        Reader::Int16(v) => Value::Integer(i128::from(v)),
        Reader::Int32(v) => Value::Integer(i128::from(v)),
        Reader::Int64(v) => Value::Integer(i128::from(v)),
        Reader::UInt8(v) => Value::Integer(i128::from(v)),
        Reader::UInt16(v) => Value::Integer(i128::from(v)),
        Reader::UInt32(v) => Value::Integer(i128::from(v)),
        Reader::UInt64(v) => Value::Integer(i128::from(v)),
        Reader::Float32(v) => Value::Float(f64::from(v)),
        Reader::Float64(v) => Value::Float(v),
        Reader::Text(v) => Value::Text(v.to_str()?.to_owned()),
        Reader::Data(v) if !in_dict => Value::Text(std::str::from_utf8(v)?.to_owned()),
        Reader::Data(_) => return Err(Error::Contract("nested bytes are not JSON serializable")),
        Reader::Enum(v) => Value::Text(
            v.get_enumerant()?
                .ok_or(Error::Contract("unknown enum"))?
                .get_proto()
                .get_name()?
                .to_str()?
                .to_owned(),
        ),
        Reader::List(v) => Value::Array(
            v.iter()
                .map(|v| value(v?, in_dict))
                .collect::<Result<_, Error>>()?,
        ),
        Reader::Struct(v) => {
            let mut fields = Fields::new();
            for field in v.get_schema().get_fields()? {
                if v.has(field)? {
                    let name = field.get_proto().get_name()?.to_str()?.to_owned();
                    fields.insert(name, value(v.get(field)?, true)?);
                }
            }
            Value::Object(fields)
        }
        Reader::AnyPointer(_) | Reader::Capability(_) => {
            return Err(Error::Contract("non-JSON cereal value"))
        }
    })
}

/// Encodes the original outgoing bridge envelope with Python JSON formatting.
///
/// # Errors
/// Returns parsing, UTF-8 or source-equivalent non-JSON payload errors.
pub fn outgoing(bytes: &[u8]) -> Result<String, Error> {
    let message = read(bytes)?;
    let root = message.get_root::<event::Reader<'_>>()?;
    outgoing_event(root)
}

pub(crate) fn outgoing_event(root: event::Reader<'_>) -> Result<String, Error> {
    let Reader::Struct(dynamic) = root.into() else {
        return Err(Error::Contract("Event must be a struct"));
    };
    let field = dynamic
        .which()?
        .ok_or(Error::Contract("Event has no active service"))?;
    let name = field.get_proto().get_name()?.to_str()?;
    let fields: Fields = [
        ("type".to_owned(), Value::Text(name.to_owned())),
        (
            "logMonoTime".to_owned(),
            Value::Integer(i128::from(root.get_log_mono_time())),
        ),
        ("valid".to_owned(), Value::Bool(root.get_valid())),
        ("data".to_owned(), value(dynamic.get(field)?, false)?),
    ]
    .into_iter()
    .collect();
    Ok(fields.to_json()?)
}
