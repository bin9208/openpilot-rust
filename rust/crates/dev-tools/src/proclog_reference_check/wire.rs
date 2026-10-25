//! Pycapnp to_dict policy, following the existing bootlog full-schema visitor.
//! Scalars include defaults; pointers follow presence and active unions only.
use super::Result;
use capnp::{dynamic_struct, dynamic_value::Reader as D};
use openpilot_cereal::log_capnp::event;
use serde_json::{json, Map, Value};
use std::io::Cursor;
fn value(input: D<'_>) -> Result<Value> {
    Ok(match input {
        D::Void => Value::Null,
        D::Bool(v) => json!(v),
        D::Int8(v) => json!(v),
        D::Int16(v) => json!(v),
        D::Int32(v) => json!(v),
        D::Int64(v) => json!(v),
        D::UInt8(v) => json!(v),
        D::UInt16(v) => json!(v),
        D::UInt32(v) => json!(v),
        D::UInt64(v) => json!(v),
        D::Float32(v) => json!(f64::from(v)),
        D::Float64(v) => json!(v),
        D::Text(v) => json!(v.to_str()?),
        D::Enum(v) => json!(v
            .get_enumerant()?
            .ok_or("unknown cereal enum")?
            .get_proto()
            .get_name()?
            .to_str()?),
        D::List(v) => Value::Array(v.iter().map(|item| value(item?)).collect::<Result<_>>()?),
        D::Struct(v) => Value::Object(structure(v)?),
        D::Data(_) => return Err("TypeError: bytes are not JSON serializable".into()),
        D::AnyPointer(_) | D::Capability(_) => return Err("unsupported cereal pointer".into()),
    })
}
fn structure(input: dynamic_struct::Reader<'_>) -> Result<Map<String, Value>> {
    let mut result = Map::new();
    if let Some(field) = input.which()? {
        result.insert(
            field.get_proto().get_name()?.to_str()?.into(),
            value(input.get(field)?)?,
        );
    } else if !input.get_schema().get_union_fields()?.is_empty() {
        return Err("unknown cereal union discriminant".into());
    }
    for field in input.get_schema().get_non_union_fields()? {
        if input.has(field)? {
            result.insert(
                field.get_proto().get_name()?.to_str()?.into(),
                value(input.get(field)?)?,
            );
        }
    }
    Ok(result)
}
pub fn project_event(bytes: &[u8]) -> Result<Value> {
    if !bytes.len().is_multiple_of(8) {
        return Err("capnp input is not word aligned".into());
    }
    let message = capnp::serialize::read_message(
        &mut Cursor::new(bytes),
        capnp::message::ReaderOptions::new(),
    )?;
    value(message.get_root::<event::Reader<'_>>()?.into())
}
pub fn compare_frame(cycle: usize, ticks: u64, page: u64, bytes: &[u8]) -> Result<()> {
    let mut actual = project_event(bytes)?;
    let procs = actual
        .get_mut("procLog")
        .and_then(|v| v.get_mut("procs"))
        .and_then(Value::as_array_mut)
        .ok_or("candidate missing procLog processes")?;
    procs.sort_by_key(|v| v["pid"].as_i64());
    let expected = super::expected_event(cycle, ticks, page)?;
    if actual != expected {
        return Err(format!(
            "procLog differs at cycle {cycle}: actual={actual}, expected={expected}"
        )
        .into());
    }
    Ok(())
}
