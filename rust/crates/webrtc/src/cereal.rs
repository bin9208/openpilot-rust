use crate::Error;
use capnp::{
    dynamic_struct,
    dynamic_value::Reader,
    introspect::{Type, TypeVariant},
    message::{self, ReaderOptions},
    serialize::{self, OwnedSegments},
};
mod outgoing;
pub use outgoing::outgoing;
#[cfg(feature = "native")]
pub(crate) use outgoing::outgoing_event;

use num_traits::ToPrimitive;
use openpilot_cereal::log_capnp::event;
use openpilot_logmessaged::{JsonValue, JsonView};
use std::io::Cursor;

pub(crate) fn read(bytes: &[u8]) -> Result<message::Reader<OwnedSegments>, Error> {
    let message = serialize::read_message(
        Cursor::new(bytes),
        ReaderOptions {
            traversal_limit_in_words: Some(bytes.len() / 8),
            nesting_limit: 64,
        },
    )?;
    Ok(message::Reader::new(
        message.into_segments(),
        ReaderOptions {
            traversal_limit_in_words: None,
            nesting_limit: 64,
        },
    ))
}

fn text(value: &JsonValue) -> Result<String, Error> {
    value
        .to_utf8()
        .ok_or(Error::Contract("expected UTF-8 text"))
}

fn integer(value: &JsonValue) -> Result<i128, Error> {
    match value.view() {
        JsonView::Integer(v) => v.parse().map_err(|_| Error::Contract("integer range")),
        JsonView::Bool(v) => Ok(i128::from(v)),
        _ => Err(Error::Contract("expected integer")),
    }
}

fn float(value: &JsonValue) -> Result<f64, Error> {
    match value.view() {
        JsonView::Float(v) => Ok(v),
        JsonView::Integer(_) | JsonView::Bool(_) => integer(value)?
            .to_f64()
            .ok_or(Error::Contract("float range")),
        _ => Err(Error::Contract("expected number")),
    }
}

fn scalar(kind: Type, input: &JsonValue) -> Result<Reader<'static>, Error> {
    Ok(match kind.which() {
        TypeVariant::Void if matches!(input.view(), JsonView::Null) => Reader::Void,
        TypeVariant::Bool => match input.view() {
            JsonView::Bool(v) => Reader::Bool(v),
            _ => return Err(Error::Contract("expected boolean")),
        },
        TypeVariant::Int8 => Reader::Int8(i8::try_from(integer(input)?)?),
        TypeVariant::Int16 => Reader::Int16(i16::try_from(integer(input)?)?),
        TypeVariant::Int32 => Reader::Int32(i32::try_from(integer(input)?)?),
        TypeVariant::Int64 => Reader::Int64(i64::try_from(integer(input)?)?),
        TypeVariant::UInt8 => Reader::UInt8(u8::try_from(integer(input)?)?),
        TypeVariant::UInt16 => Reader::UInt16(u16::try_from(integer(input)?)?),
        TypeVariant::UInt32 => Reader::UInt32(u32::try_from(integer(input)?)?),
        TypeVariant::UInt64 => Reader::UInt64(u64::try_from(integer(input)?)?),
        TypeVariant::Float32 => Reader::Float32(
            float(input)?
                .to_f32()
                .ok_or(Error::Contract("float32 range"))?,
        ),
        TypeVariant::Float64 => Reader::Float64(float(input)?),
        TypeVariant::Enum(schema) => {
            let schema: capnp::schema::EnumSchema = schema.into();
            let ordinal = if let Some(name) = input.to_utf8() {
                let mut found = None;
                for candidate in schema.get_enumerants()? {
                    if candidate.get_proto().get_name()?.to_str()? == name {
                        found = Some(candidate.get_ordinal());
                        break;
                    }
                }
                found.ok_or(Error::Contract("invalid enumerant"))?
            } else {
                u16::try_from(integer(input)?)?
            };
            Reader::Enum(capnp::dynamic_value::Enum::new(ordinal, schema))
        }
        _ => return Err(Error::Contract("expected scalar field")),
    })
}

fn list(mut target: capnp::dynamic_list::Builder<'_>, values: &[JsonValue]) -> Result<(), Error> {
    let kind = target.reborrow().into_reader().element_type();
    for (index, input) in values.iter().enumerate() {
        let index = u32::try_from(index)?;
        match kind.which() {
            TypeVariant::Struct(_) => {
                let capnp::dynamic_value::Builder::Struct(child) = target.reborrow().get(index)?
                else {
                    return Err(Error::Contract("list element must be a struct"));
                };
                structure(child, input)?;
            }
            TypeVariant::List(_) => {
                let JsonView::Array(values) = input.view() else {
                    return Err(Error::Contract("expected list"));
                };
                let capnp::dynamic_value::Builder::List(child) = target
                    .reborrow()
                    .init(index, u32::try_from(values.len())?)?
                else {
                    return Err(Error::Contract("list element must be a list"));
                };
                list(child, &values)?;
            }
            TypeVariant::Text | TypeVariant::Data => {
                let text = text(input)?;
                target.set(
                    index,
                    if kind.which() == TypeVariant::Text {
                        Reader::Text(text.as_str().into())
                    } else {
                        Reader::Data(text.as_bytes())
                    },
                )?;
            }
            _ => target.set(index, scalar(kind, input)?)?,
        }
    }
    Ok(())
}

fn set_field(
    mut target: dynamic_struct::Builder<'_>,
    field: capnp::schema::Field,
    input: &JsonValue,
) -> Result<(), Error> {
    match field.get_type().which() {
        TypeVariant::Struct(_) => {
            let capnp::dynamic_value::Builder::Struct(child) = target.init(field)? else {
                return Err(Error::Contract("expected struct"));
            };
            structure(child, input)?;
        }
        TypeVariant::List(_) => {
            let JsonView::Array(values) = input.view() else {
                return Err(Error::Contract("expected list"));
            };
            let capnp::dynamic_value::Builder::List(child) =
                target.initn(field, u32::try_from(values.len())?)?
            else {
                return Err(Error::Contract("expected list"));
            };
            list(child, &values)?;
        }
        TypeVariant::Text | TypeVariant::Data => {
            let text = text(input)?;
            target.set(
                field,
                if field.get_type().which() == TypeVariant::Text {
                    Reader::Text(text.as_str().into())
                } else {
                    Reader::Data(text.as_bytes())
                },
            )?;
        }
        _ => target.set(field, scalar(field.get_type(), input)?)?,
    }
    Ok(())
}

fn structure(mut target: dynamic_struct::Builder<'_>, input: &JsonValue) -> Result<(), Error> {
    let JsonView::Object(fields) = input.view() else {
        return Err(Error::Contract("expected object"));
    };
    for (name, input) in fields {
        let name: String = name
            .iter()
            .copied()
            .map(char::from_u32)
            .collect::<Option<_>>()
            .ok_or(Error::Contract("invalid field name Unicode"))?;
        let field = target.get_schema().get_field_by_name(&name)?;
        set_field(target.reborrow(), field, &input)?;
    }
    Ok(())
}

/// Builds an incoming Event, ignoring the client's valid and timestamp fields.
///
/// # Errors
/// Rejects malformed JSON, unknown services and values rejected by the schema.
pub fn incoming(source: &str, monotonic_ns: u64) -> Result<(String, Vec<u8>), Error> {
    let input = JsonValue::parse(source)?;
    let service = text(&input.get("type").ok_or(Error::Contract("missing type"))?)?;
    let data = input.get("data").ok_or(Error::Contract("missing data"))?;
    let mut message = message::Builder::new_default();
    let mut root = message.init_root::<event::Builder<'_>>();
    root.set_log_mono_time(monotonic_ns);
    root.set_valid(false);
    let capnp::dynamic_value::Builder::Struct(mut target) = root.into() else {
        return Err(Error::Contract("Event must be a struct"));
    };
    let field = target.get_schema().get_field_by_name(&service)?;
    set_field(target.reborrow(), field, &data)?;
    Ok((service, serialize::write_message_to_words(&message)))
}
