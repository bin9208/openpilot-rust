use crate::Error;
use capnp::{
    dynamic_struct,
    dynamic_value::{self, Reader},
    introspect::TypeVariant,
};
use openpilot_cereal::log_capnp::event;
use serde_json::{json, Value};

pub fn json_value(value: Reader<'_>) -> Result<Value, Error> {
    Ok(match value {
        Reader::Void => Value::Null,
        Reader::Bool(v) => json!(v),
        Reader::Int8(v) => json!(v),
        Reader::Int16(v) => json!(v),
        Reader::Int32(v) => json!(v),
        Reader::Int64(v) => json!(v),
        Reader::UInt8(v) => json!(v),
        Reader::UInt16(v) => json!(v),
        Reader::UInt32(v) => json!(v),
        Reader::UInt64(v) => json!(v),
        Reader::Float32(v) => json!(v),
        Reader::Float64(v) => json!(v),
        Reader::Text(v) => json!(v
            .to_str()
            .map_err(|_| Error::Contract("invalid cereal text"))?),
        Reader::Data(v) => json!(String::from_utf8_lossy(v)),
        Reader::Enum(v) => match v.get_enumerant()? {
            Some(e) => json!(e
                .get_proto()
                .get_name()?
                .to_str()
                .map_err(|_| Error::Contract("invalid enum name"))?),
            None => json!(v.get_value()),
        },
        Reader::List(v) => Value::Array(
            v.iter()
                .map(|v| json_value(v?))
                .collect::<Result<_, Error>>()?,
        ),
        Reader::Struct(v) => {
            let mut map = serde_json::Map::new();
            for field in v
                .get_schema()
                .get_non_union_fields()?
                .iter()
                .chain(v.which()?.into_iter())
            {
                let name = field
                    .get_proto()
                    .get_name()?
                    .to_str()
                    .map_err(|_| Error::Contract("invalid field name"))?;
                if v.has(field)? && name != "deprecated" && !name.ends_with("DEPRECATED") {
                    map.insert(name.into(), json_value(v.get(field)?)?);
                }
            }
            Value::Object(map)
        }
        Reader::AnyPointer(_) | Reader::Capability(_) => {
            return Err(Error::Contract("unsupported status packet field"))
        }
    })
}
fn number(value: &Value) -> Result<f64, Error> {
    value
        .as_f64()
        .ok_or(Error::Contract("expected numeric cereal field"))
}
fn signed(value: &Value) -> Result<i64, Error> {
    if let Some(integer) = value.as_i64() {
        return Ok(integer);
    }
    let value = number(value)?.trunc();
    if !(-9223372036854775808.0..9223372036854775808.0).contains(&value) {
        return Err(Error::Contract("signed cereal integer out of range"));
    }
    Ok(value as i64)
}
fn unsigned(value: &Value) -> Result<u64, Error> {
    if let Some(integer) = value.as_u64() {
        return Ok(integer);
    }
    let value = number(value)?.trunc();
    if !(0.0..18446744073709551616.0).contains(&value) {
        return Err(Error::Contract("unsigned cereal integer out of range"));
    }
    Ok(value as u64)
}
fn scalar<'a>(value: &'a Value, kind: TypeVariant) -> Result<Reader<'a>, Error> {
    Ok(match kind {
        TypeVariant::Bool => {
            Reader::Bool(value.as_bool().ok_or(Error::Contract("expected boolean"))?)
        }
        TypeVariant::Int8 => Reader::Int8(
            i8::try_from(signed(value)?).map_err(|_| Error::Contract("Int8 out of range"))?,
        ),
        TypeVariant::Int16 => Reader::Int16(
            i16::try_from(signed(value)?).map_err(|_| Error::Contract("Int16 out of range"))?,
        ),
        TypeVariant::Int32 => Reader::Int32(
            i32::try_from(signed(value)?).map_err(|_| Error::Contract("Int32 out of range"))?,
        ),
        TypeVariant::Int64 => Reader::Int64(signed(value)?),
        TypeVariant::UInt8 => Reader::UInt8(
            u8::try_from(unsigned(value)?).map_err(|_| Error::Contract("UInt8 out of range"))?,
        ),
        TypeVariant::UInt16 => Reader::UInt16(
            u16::try_from(unsigned(value)?).map_err(|_| Error::Contract("UInt16 out of range"))?,
        ),
        TypeVariant::UInt32 => Reader::UInt32(
            u32::try_from(unsigned(value)?).map_err(|_| Error::Contract("UInt32 out of range"))?,
        ),
        TypeVariant::UInt64 => Reader::UInt64(unsigned(value)?),
        TypeVariant::Float32 => Reader::Float32(number(value)? as f32),
        TypeVariant::Float64 => Reader::Float64(number(value)?),
        TypeVariant::Text => Reader::Text(
            value
                .as_str()
                .ok_or(Error::Contract("expected text"))?
                .into(),
        ),
        TypeVariant::Enum(raw_schema) => {
            let schema: capnp::schema::EnumSchema = raw_schema.into();
            let ordinal = if let Some(name) = value.as_str() {
                let mut ordinal = None;
                for e in schema.get_enumerants()? {
                    if e.get_proto()
                        .get_name()?
                        .to_str()
                        .map_err(|_| Error::Contract("enum utf8"))?
                        == name
                    {
                        ordinal = Some(e.get_ordinal());
                        break;
                    }
                }
                ordinal.ok_or(Error::Contract("unknown enum"))?
            } else {
                u16::try_from(value.as_u64().ok_or(Error::Contract("enum ordinal"))?)
                    .map_err(|_| Error::Contract("enum overflow"))?
            };
            Reader::Enum(dynamic_value::Enum::new(ordinal, schema))
        }
        _ => return Err(Error::Contract("expected scalar field")),
    })
}
pub fn fill(mut target: dynamic_struct::Builder<'_>, value: &Value) -> Result<(), Error> {
    for (name, value) in value
        .as_object()
        .ok_or(Error::Contract("expected object"))?
    {
        let field = target.get_schema().get_field_by_name(name)?;
        match field.get_type().which() {
            TypeVariant::Struct(_) => {
                let dynamic_value::Builder::Struct(child) = target.reborrow().init(field)? else {
                    return Err(Error::Contract("struct field"));
                };
                fill(child, value)?;
            }
            TypeVariant::List(element) => {
                let values = value.as_array().ok_or(Error::Contract("expected list"))?;
                let dynamic_value::Builder::List(mut child) = target.reborrow().initn(
                    field,
                    u32::try_from(values.len()).map_err(|_| Error::Contract("list too long"))?,
                )?
                else {
                    return Err(Error::Contract("list field"));
                };
                for (index, value) in values.iter().enumerate() {
                    child.set(index as u32, scalar(value, element.which())?)?;
                }
            }
            kind => target.set(field, scalar(value, kind)?)?,
        }
    }
    Ok(())
}
pub fn device(value: &Value, now: f64) -> Result<(Vec<u8>, Value), Error> {
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<event::Builder>();
    event.set_log_mono_time((now * 1e9) as u64);
    event.set_valid(true);
    let dynamic_value::Builder::Struct(target) = event.reborrow().init_device_state().into() else {
        return Err(Error::Contract("device state schema"));
    };
    fill(target, value)?;
    let full = json_value(event.into_reader().into())?;
    Ok((capnp::serialize::write_message_to_words(&message), full))
}
