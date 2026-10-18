//! Full generated cereal schema projection for the native encoder comparator.
//! Raw captures precede validation, preserving successful rows before a bad frame.
use crate::seal::Result;
use capnp::{dynamic_struct, dynamic_value::Reader as Dynamic};
use openpilot_cereal::log_capnp::event;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Write};
fn project(value: Dynamic<'_>) -> Result<Value> {
    Ok(match value {
        Dynamic::Void => Value::Null,
        Dynamic::Bool(v) => json!(v),
        Dynamic::Int8(v) => json!(v),
        Dynamic::Int16(v) => json!(v),
        Dynamic::Int32(v) => json!(v),
        Dynamic::Int64(v) => json!(v),
        Dynamic::UInt8(v) => json!(v),
        Dynamic::UInt16(v) => json!(v),
        Dynamic::UInt32(v) => json!(v),
        Dynamic::UInt64(v) => json!(v),
        Dynamic::Float32(v) => json!(v),
        Dynamic::Float64(v) => json!(v),
        Dynamic::Text(v) => json!(v.to_str()?),
        Dynamic::Enum(v) => match v.get_enumerant()? {
            Some(e) => json!(e.get_proto().get_name()?.to_str()?),
            None => return Err("Member was null.".into()),
        },
        Dynamic::Struct(v) => Value::Object(structure(v)?),
        Dynamic::List(v) => Value::Array(v.iter().map(|v| project(v?)).collect::<Result<_>>()?),
        Dynamic::Data(_) => return Err("Object of type bytes is not JSON serializable".into()),
        Dynamic::AnyPointer(_) | Dynamic::Capability(_) => {
            return Err("unsupported encoder schema value".into())
        }
    })
}
fn structure(value: dynamic_struct::Reader<'_>) -> Result<Map<String, Value>> {
    let mut out = Map::new();
    if let Some(field) = value.which()? {
        out.insert(
            field.get_proto().get_name()?.to_str()?.to_owned(),
            project(value.get(field)?)?,
        );
    }
    for field in value.get_schema().get_non_union_fields()? {
        if value.has(field)? {
            out.insert(
                field.get_proto().get_name()?.to_str()?.to_owned(),
                project(value.get(field)?)?,
            );
        }
    }
    Ok(out)
}
fn data<'a>(packet: dynamic_struct::Reader<'a>, field: &str, optional: bool) -> Result<&'a [u8]> {
    if !packet.has_named(field)? {
        return if optional {
            Ok(&[])
        } else {
            Err(format!("'{field}'").into())
        };
    }
    match packet.get_named(field)? {
        Dynamic::Data(v) => Ok(v),
        _ => Err("encoder payload is not Data".into()),
    }
}
fn packet(event: event::Reader<'_>, service: &str) -> Result<Value> {
    let Dynamic::Struct(envelope) = Dynamic::from(event) else {
        unreachable!()
    };
    let union = envelope.which()?.ok_or("missing Event union")?;
    if union.get_proto().get_name()?.to_str()? != service
        || !event.get_valid()
        || event.get_log_mono_time() == 0
    {
        return Err(format!("invalid publication envelope: {service}").into());
    }
    let Dynamic::Struct(packet) = envelope.get(union)? else {
        return Err("encoder publication is not a struct".into());
    };
    let mut row = Map::new();
    // The original to_dict reads every present field before validation/removal.
    // Retain all schema fields rather than a hand-selected metadata subset.
    for field in packet.get_schema().get_non_union_fields()? {
        let name = field.get_proto().get_name()?.to_str()?;
        if packet.has(field)? {
            let value = packet.get(field)?;
            if matches!(name, "data" | "header" | "thumbnail") && matches!(value, Dynamic::Data(_))
            {
                continue;
            }
            row.insert(name.to_owned(), project(value)?);
        }
    }
    if service == "thumbnail" {
        row.insert(
            "thumbnail_sha256".into(),
            json!(format!(
                "{:x}",
                Sha256::digest(data(packet, "thumbnail", false)?)
            )),
        );
    } else {
        if row
            .remove("unixTimestampNanos")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            == 0
        {
            return Err("missing wall timestamp".into());
        }
        let payload = data(packet, "data", false)?;
        let header = data(packet, "header", true)?;
        let idx = row.get("idx").ok_or("'idx'")?;
        if idx["len"].as_u64() != Some(payload.len() as u64) {
            return Err("incorrect encoded length".into());
        }
        if idx["flags"].as_u64().ok_or("missing flags")? & 8 == 0 && !header.is_empty() {
            return Err("header on a non-keyframe".into());
        }
        row.insert(
            "data_sha256".into(),
            json!(format!("{:x}", Sha256::digest(payload))),
        );
        row.insert(
            "header_sha256".into(),
            json!(format!("{:x}", Sha256::digest(header))),
        );
    }
    Ok(Value::Object(row))
}
pub fn drain(
    service: &str,
    wires: &[Vec<u8>],
    raw: &mut dyn Write,
    rows: &mut Vec<Value>,
) -> Result<()> {
    for wire in wires {
        raw.write_all(&u32::try_from(wire.len())?.to_le_bytes())?;
        raw.write_all(wire)?;
        let message = capnp::serialize::read_message(
            Cursor::new(wire),
            capnp::message::ReaderOptions::new(),
        )?;
        rows.push(packet(message.get_root::<event::Reader<'_>>()?, service)?);
    }
    Ok(())
}
