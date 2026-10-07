use crate::{json::Value, projection, record::Record, Error};
use capnp::{dynamic_struct, dynamic_value, introspect::TypeVariant};
use num_traits::ToPrimitive;
use openpilot_cereal::log_capnp::event;

fn capnp(error: capnp::Error) -> Error {
    Error::typed("KjException", error.to_string())
}
fn range() -> Error {
    Error::typed(
        "OverflowError",
        "Python int too large to convert to C unsigned long".into(),
    )
}

fn scalar(value: &Value, kind: TypeVariant) -> Result<dynamic_value::Reader<'_>, Error> {
    use dynamic_value::Reader as R;
    Ok(match kind {
        TypeVariant::Bool => R::Bool(value.truth()),
        TypeVariant::Int8 => R::Int8(value.int()?.to_i8().ok_or_else(range)?),
        TypeVariant::Int16 => R::Int16(value.int()?.to_i16().ok_or_else(range)?),
        TypeVariant::Int32 => R::Int32(value.int()?.to_i32().ok_or_else(range)?),
        TypeVariant::Int64 => R::Int64(value.int()?.to_i64().ok_or_else(range)?),
        TypeVariant::UInt8 => R::UInt8(value.int()?.to_u8().ok_or_else(range)?),
        TypeVariant::UInt16 => R::UInt16(value.int()?.to_u16().ok_or_else(range)?),
        TypeVariant::UInt32 => R::UInt32(value.int()?.to_u32().ok_or_else(range)?),
        TypeVariant::UInt64 => R::UInt64(value.int()?.to_u64().ok_or_else(range)?),
        TypeVariant::Float32 => R::Float32(value.float()? as f32),
        TypeVariant::Float64 => R::Float64(value.float()?),
        TypeVariant::Void
        | TypeVariant::Text
        | TypeVariant::Data
        | TypeVariant::Struct(_)
        | TypeVariant::AnyPointer
        | TypeVariant::Capability
        | TypeVariant::Enum(_)
        | TypeVariant::List(_) => {
            return Err(Error::typed(
                "RuntimeError",
                "unexpected Cereal scalar type".into(),
            ))
        }
    })
}

fn fill(mut output: dynamic_struct::Builder<'_>, value: &Value) -> Result<(), Error> {
    let Value::Object(fields) = value else {
        return Err(Error::typed(
            "TypeError",
            "Cereal struct payload must be an object".into(),
        ));
    };
    for (name, value) in fields {
        let name = crate::json_log::utf8(name)?;
        let field = output
            .get_schema()
            .get_field_by_name(&name)
            .map_err(capnp)?;
        match field.get_type().which() {
            TypeVariant::Struct(_) => {
                let dynamic_value::Builder::Struct(child) =
                    output.reborrow().init(field).map_err(capnp)?
                else {
                    return Err(Error::value("unexpected Cereal struct builder"));
                };
                fill(child, value)?;
            }
            TypeVariant::List(_) => {
                let Value::Array(values) = value else {
                    return Err(Error::typed(
                        "TypeError",
                        "Cereal list payload must be an array".into(),
                    ));
                };
                let length = u32::try_from(values.len()).map_err(|_| range())?;
                let dynamic_value::Builder::List(mut list) =
                    output.reborrow().initn(field, length).map_err(capnp)?
                else {
                    return Err(Error::value("unexpected Cereal list builder"));
                };
                for (index, value) in (0..length).zip(values) {
                    match list.element_type().which() {
                        TypeVariant::Struct(_) => {
                            let dynamic_value::Builder::Struct(child) =
                                list.reborrow().get(index).map_err(capnp)?
                            else {
                                return Err(Error::value("unexpected Cereal list element"));
                            };
                            fill(child, value)?;
                        }
                        kind => list.set(index, scalar(value, kind)?).map_err(capnp)?,
                    }
                }
            }
            TypeVariant::Text => {
                let Value::Text(points) = value else {
                    return Err(Error::typed(
                        "TypeError",
                        "Cereal text payload must be a string".into(),
                    ));
                };
                let text = crate::json_log::utf8(points)?;
                output
                    .set(field, dynamic_value::Reader::Text(text.as_str().into()))
                    .map_err(capnp)?;
            }
            kind => output.set(field, scalar(value, kind)?).map_err(capnp)?,
        }
    }
    Ok(())
}

pub fn state(snapshot: &Value) -> Result<Vec<u8>, Error> {
    state_at(snapshot, (super::clock::seconds() * 1e9) as u64, || {
        super::clock::nanos(rustix::time::ClockId::Monotonic)
    })
}

pub fn state_at(
    snapshot: &Value,
    event_ns: u64,
    publish_ns: impl FnOnce() -> u128,
) -> Result<Vec<u8>, Error> {
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<event::Builder<'_>>();
    event.set_valid(true);
    event.set_log_mono_time(event_ns);
    let body = event.init_carrot_navi();
    let payload = projection::payload(snapshot, publish_ns)?;
    let dynamic_value::Builder::Struct(body) = body.into() else {
        return Err(Error::value("unexpected Navi builder"));
    };
    fill(body, &payload)?;
    Ok(capnp::serialize::write_message_to_words(&message))
}

pub fn media(record: &Record, session: &Value, kind: Option<&str>) -> Result<Vec<u8>, Error> {
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<event::Builder<'_>>();
    event.set_valid(true);
    event.set_log_mono_time((super::clock::seconds() * 1e9) as u64);
    let dynamic_value::Builder::Struct(mut body) = event.init_carrot_navi_media().into() else {
        return Err(Error::value("unexpected media builder"));
    };
    let mut payload = projection::media(record, session)?;
    if let (Some(kind), Value::Object(fields)) = (kind, &mut payload.metadata) {
        if let Some((_, value)) = fields
            .iter_mut()
            .find(|(name, _)| name.iter().copied().eq("kind".chars().map(u32::from)))
        {
            *value = Value::text(kind);
        }
    }
    fill(body.reborrow(), &payload.metadata)?;
    body.set_named("payload", dynamic_value::Reader::Data(&payload.payload))
        .map_err(capnp)?;
    Ok(capnp::serialize::write_message_to_words(&message))
}
