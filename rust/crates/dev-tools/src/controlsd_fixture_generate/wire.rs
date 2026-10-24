//! Full-schema wire mechanics for the closed controlsd fixture service roster.
use super::{array, bytes, field, number, object, text, Result, J, V};
use capnp::{
    dynamic_list, dynamic_struct,
    dynamic_value::{Builder as B, Reader as D},
    introspect::{Type, TypeVariant},
    message::ReaderOptions,
    serialize,
};
use openpilot_cereal::log_capnp::event;

fn integer(value: &J) -> Result<i128> {
    match value.view() {
        V::Integer(n) => Ok(n.parse()?),
        V::Bool(n) => Ok(i128::from(n)),
        _ => Err("fixture integer field requires an integer".into()),
    }
}
fn scalar<'a>(value: &J, ty: Type) -> Result<D<'a>> {
    Ok(match ty.which() {
        TypeVariant::Void => D::Void,
        TypeVariant::Bool => D::Bool(match value.view() {
            V::Bool(b) => b,
            _ => integer(value)? != 0,
        }),
        TypeVariant::Int8 => D::Int8(integer(value)?.try_into()?),
        TypeVariant::Int16 => D::Int16(integer(value)?.try_into()?),
        TypeVariant::Int32 => D::Int32(match value.view() {
            // Original Carrot fixtures assign whole-valued floats to Int32 fields.
            V::Float(n)
                if n.is_finite()
                    && n.fract() == 0.
                    && (f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&n) =>
            {
                n as i32
            }
            _ => integer(value)?.try_into()?,
        }),
        TypeVariant::Int64 => D::Int64(integer(value)?.try_into()?),
        TypeVariant::UInt8 => D::UInt8(integer(value)?.try_into()?),
        TypeVariant::UInt16 => D::UInt16(integer(value)?.try_into()?),
        TypeVariant::UInt32 => D::UInt32(integer(value)?.try_into()?),
        TypeVariant::UInt64 => D::UInt64(integer(value)?.try_into()?),
        TypeVariant::Float32 => D::Float32(number(value)? as f32),
        TypeVariant::Float64 => D::Float64(number(value)?),
        TypeVariant::Enum(raw) => {
            let schema: capnp::schema::EnumSchema = raw.into();
            let value = if let Some(name) = value.to_utf8() {
                let mut found = None;
                for enumerant in schema.get_enumerants()? {
                    if enumerant.get_proto().get_name()?.to_str()? == name {
                        found = Some(enumerant.get_ordinal());
                        break;
                    }
                }
                found.ok_or("unknown fixture enum")?
            } else {
                integer(value)?.try_into()?
            };
            D::Enum(capnp::dynamic_value::Enum::new(value, schema))
        }
        _ => return Err("fixture scalar/schema type mismatch".into()),
    })
}
fn sequence(value: &J) -> Result<Vec<J>> {
    match value.view() {
        V::Array(rows) => Ok(rows),
        _ => Err("fixture list requires an array".into()),
    }
}
fn list(mut target: dynamic_list::Builder<'_>, ty: Type, values: &[J]) -> Result<()> {
    for (index, value) in values.iter().enumerate() {
        let index = u32::try_from(index)?;
        match ty.which() {
            TypeVariant::Struct(_) => {
                let B::Struct(target) = target.reborrow().get(index)? else {
                    return Err("expected list struct".into());
                };
                fill(target, value)?;
            }
            TypeVariant::List(inner) => {
                let rows = sequence(value)?;
                let B::List(target) = target.reborrow().init(index, rows.len().try_into()?)? else {
                    return Err("expected nested list".into());
                };
                list(target, inner, &rows)?;
            }
            TypeVariant::Text => {
                let value = text(value)?;
                target.set(index, D::Text(value.as_str().into()))?;
            }
            TypeVariant::Data => {
                let value = bytes(value)?;
                target.set(index, D::Data(&value))?;
            }
            _ => target.set(index, scalar(value, ty)?)?,
        }
    }
    Ok(())
}
pub(super) fn fill(mut target: dynamic_struct::Builder<'_>, value: &J) -> Result<()> {
    let V::Object(fields) = value.view() else {
        return Err("fixture struct requires an object".into());
    };
    // Preserve the source dictionary's insertion order (including allocation order).
    for (name, value) in fields {
        let name: String = name
            .iter()
            .copied()
            .map(|c| char::from_u32(c).ok_or("invalid field text"))
            .collect::<std::result::Result<_, _>>()?;
        let slot = target.get_schema().get_field_by_name(&name)?;
        match slot.get_type().which() {
            TypeVariant::Struct(_) => {
                let B::Struct(child) = target.reborrow().init(slot)? else {
                    return Err("expected fixture child struct".into());
                };
                fill(child, &value)?;
            }
            TypeVariant::List(inner) => {
                let rows = sequence(&value)?;
                let B::List(child) = target.reborrow().initn(slot, rows.len().try_into()?)? else {
                    return Err("expected fixture child list".into());
                };
                list(child, inner, &rows)?;
            }
            TypeVariant::Text => {
                let value = text(&value)?;
                target.set(slot, D::Text(value.as_str().into()))?;
            }
            TypeVariant::Data => {
                let value = bytes(&value)?;
                target.set(slot, D::Data(&value))?;
            }
            _ => target.set(slot, scalar(&value, slot.get_type())?)?,
        }
    }
    Ok(())
}
pub(super) fn event_packet(service: &str, time: f64, data: &J) -> Result<Vec<u8>> {
    if !super::SERVICES.contains(&service) {
        return Err("service outside controlsd fixture roster".into());
    }
    let stamp = (time * 1e9).trunc();
    if !(0.0..18_446_744_073_709_551_616.0).contains(&stamp) {
        return Err("fixture timestamp outside UInt64".into());
    }
    let mut message = capnp::message::Builder::new_default();
    let mut root = message.init_root::<event::Builder<'_>>();
    root.set_log_mono_time(stamp as u64);
    root.set_valid(true);
    let B::Struct(mut dynamic) = root.into() else {
        return Err("Event is not a struct".into());
    };
    if service == "onroadEvents" {
        let rows = sequence(data)?;
        let slot = dynamic.get_schema().get_field_by_name(service)?;
        let TypeVariant::List(inner) = slot.get_type().which() else {
            return Err("onroadEvents not list".into());
        };
        let B::List(target) = dynamic.reborrow().initn(slot, rows.len().try_into()?)? else {
            return Err("onroadEvents builder not list".into());
        };
        list(target, inner, &rows)?;
    } else {
        let B::Struct(child) = dynamic.init_named(service)? else {
            return Err("fixture service not struct".into());
        };
        fill(child, data)?;
    }
    Ok(serialize::write_message_to_words(&message))
}
pub(super) fn project(value: D<'_>) -> Result<J> {
    Ok(match value {
        D::Void => J::parse("null")?,
        D::Bool(v) => J::parse(if v { "true" } else { "false" })?,
        D::Int8(v) => J::parse(&v.to_string())?,
        D::Int16(v) => J::parse(&v.to_string())?,
        D::Int32(v) => J::parse(&v.to_string())?,
        D::Int64(v) => J::parse(&v.to_string())?,
        D::UInt8(v) => J::parse(&v.to_string())?,
        D::UInt16(v) => J::parse(&v.to_string())?,
        D::UInt32(v) => J::parse(&v.to_string())?,
        D::UInt64(v) => J::parse(&v.to_string())?,
        D::Float32(v) => super::real(f64::from(v))?,
        D::Float64(v) => super::real(v)?,
        D::Enum(v) => J::text(
            v.get_enumerant()?
                .ok_or("unknown fixture enum value")?
                .get_proto()
                .get_name()?
                .to_str()?,
        ),
        D::Text(v) => J::text(v.to_str()?),
        D::Data(v) => object(vec![(
            "$data".into(),
            J::text(&v.iter().map(|v| format!("{v:02x}")).collect::<String>()),
        )])?,
        D::List(v) => array(v.iter().map(|v| project(v?)).collect::<Result<Vec<_>>>()?)?,
        D::Struct(v) => {
            let mut out = Vec::new();
            if let Some(slot) = v.which()? {
                out.push((
                    slot.get_proto().get_name()?.to_str()?.into(),
                    project(v.get(slot)?)?,
                ));
            } else if !v.get_schema().get_union_fields()?.is_empty() {
                return Err("unknown fixture union".into());
            }
            for slot in v.get_schema().get_non_union_fields()? {
                if v.has(slot)? {
                    out.push((
                        slot.get_proto().get_name()?.to_str()?.into(),
                        project(v.get(slot)?)?,
                    ));
                }
            }
            object(out)?
        }
        _ => return Err("unsupported pointer outside captured fixture domain".into()),
    })
}
pub fn project_event(packet: &[u8]) -> Result<J> {
    let mut input = packet;
    let message = serialize::read_message(&mut input, ReaderOptions::new())?;
    if !input.is_empty() {
        return Err("trailing fixture Event bytes".into());
    }
    project(message.get_root::<event::Reader<'_>>()?.into())
}
pub fn service(packet: &[u8]) -> Result<String> {
    let mut input = packet;
    let message = serialize::read_message(&mut input, ReaderOptions::new())?;
    if !input.is_empty() {
        return Err("trailing fixture Event bytes".into());
    }
    let D::Struct(value) = message.get_root::<event::Reader<'_>>()?.into() else {
        return Err("Event not struct".into());
    };
    Ok(value
        .which()?
        .ok_or("Event missing union")?
        .get_proto()
        .get_name()?
        .to_str()?
        .into())
}
pub(super) fn change(row: &J, updates: &J, omit: &[String], settings: Option<&J>) -> Result<J> {
    let mut packets = Vec::new();
    let V::Array(messages) = field(row, "messages")?.view() else {
        return Err("messages not a list".into());
    };
    for packet in messages {
        let raw = bytes(&packet)?;
        let name = service(&raw)?;
        if omit.contains(&name) {
            continue;
        }
        if let Some(updates) = updates.get(&name) {
            if name == "onroadEvents" {
                return Err("original event helper cannot rebuild the onroadEvents list".into());
            }
            let old = field(&project_event(&raw)?, &name)?;
            let merged = super::merge(&old, &updates)?;
            packets.push(super::json(&event_packet(
                &name,
                number(&field(row, "time")?)?,
                &merged,
            )?)?);
        } else {
            packets.push(packet);
        }
    }
    let mut out = super::merge(row, &object(vec![("messages".into(), array(packets)?)])?)?;
    if let Some(settings) = settings {
        if !super::entries(settings)?.is_empty() {
            let mut values = Vec::new();
            for (key, value) in super::entries(settings)? {
                values.push((key, super::json(super::python_string(&value)?.as_bytes())?));
            }
            out = super::merge(&out, &object(vec![("params".into(), object(values)?)])?)?;
        }
    }
    Ok(out)
}
