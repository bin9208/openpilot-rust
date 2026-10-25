//! 기존 driving_daemon_reference의 원래 full-schema dynamic projection 패턴.
//! Planner 비교의 float는 원래 difference()처럼 bits 전체를 비교하며 tolerance를 추가하지 않는다.
use crate::seal::Result;
use capnp::{dynamic_struct, dynamic_value::Reader as Dynamic};
use openpilot_cereal::log_capnp::event;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::io::Cursor;

#[derive(Clone, Serialize, Deserialize)]
pub struct Record {
    pub service: String,
    pub valid: bool,
    pub data: Value,
    pub timestamp: u64,
    pub path: String,
}

fn number(value: f64) -> Value {
    if value.is_finite() {
        json!(value)
    } else {
        json!({"float64_bits":format!("{:016x}", value.to_bits())})
    }
}
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
        Dynamic::Float32(v) => number(f64::from(v)),
        Dynamic::Float64(v) => number(v),
        Dynamic::Text(v) => json!(v.to_str()?),
        Dynamic::Data(v) => {
            json!({"bytes_hex":v.iter().map(|b| format!("{b:02x}")).collect::<String>()})
        }
        Dynamic::Enum(v) => json!(v
            .get_enumerant()?
            .ok_or("unknown Cereal enum")?
            .get_proto()
            .get_name()?
            .to_str()?),
        Dynamic::List(v) => {
            Value::Array(v.iter().map(|item| project(item?)).collect::<Result<_>>()?)
        }
        Dynamic::Struct(v) => Value::Object(structure(v)?),
        Dynamic::AnyPointer(_) | Dynamic::Capability(_) => {
            return Err("unsupported Cereal pointer".into())
        }
    })
}
fn structure(value: dynamic_struct::Reader<'_>) -> Result<Map<String, Value>> {
    let mut result = Map::new();
    if let Some(field) = value.which()? {
        result.insert(
            field.get_proto().get_name()?.to_str()?.into(),
            project(value.get(field)?)?,
        );
    } else if !value.get_schema().get_union_fields()?.is_empty() {
        return Err("unknown Cereal union".into());
    }
    for field in value.get_schema().get_non_union_fields()? {
        if value.has(field)? {
            result.insert(
                field.get_proto().get_name()?.to_str()?.into(),
                project(value.get(field)?)?,
            );
        }
    }
    Ok(result)
}
pub fn packet(bytes: &[u8], service: &str, path: String) -> Result<Record> {
    let message = capnp::serialize::read_message(
        &mut Cursor::new(bytes),
        capnp::message::ReaderOptions::new(),
    )?;
    let event = message.get_root::<event::Reader<'_>>()?;
    let valid = event.get_valid();
    let timestamp = event.get_log_mono_time();
    let root = project(event.into())?;
    let mut data = root
        .get(service)
        .ok_or("C++ peer publication union differs")?
        .clone();
    let fields = data
        .as_object_mut()
        .ok_or("Planner publication is not a struct")?;
    for field in [
        "processingDelay",
        "solverExecutionTime",
        "plannerExecutionTime",
        "fastRadarExecutionTime",
    ] {
        fields.remove(field);
    }
    Ok(Record {
        service: service.into(),
        valid,
        data,
        timestamp,
        path,
    })
}
fn float_hex(value: f64) -> String {
    let bits = value.to_bits();
    let sign = if bits >> 63 == 1 { "-" } else { "" };
    if value == 0.0 {
        return format!("{sign}0x0.0p+0");
    }
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & 0x000f_ffff_ffff_ffff;
    if exponent == 0 {
        format!("{sign}0x0.{fraction:013x}p-1022")
    } else {
        format!("{sign}0x1.{fraction:013x}p{:+}", exponent - 1023)
    }
}
fn compare(expected: &Value, actual: &Value, path: &str, rows: &mut Vec<Value>) {
    if rows.len() >= 50 {
        return;
    }
    match (expected, actual) {
        (Value::Object(a), Value::Object(b)) => {
            if a.len() != b.len() || a.keys().any(|key| !b.contains_key(key)) {
                let mut expected_keys = a.keys().collect::<Vec<_>>();
                let mut actual_keys = b.keys().collect::<Vec<_>>();
                expected_keys.sort();
                actual_keys.sort();
                rows.push(
                    json!({"path":path,"expected_keys":expected_keys,"actual_keys":actual_keys}),
                );
                return;
            }
            for (key, a) in a {
                compare(a, &b[key], &format!("{path}.{key}"), rows);
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                rows.push(json!({"path":path,"expected_length":a.len(),"actual_length":b.len()}));
                return;
            }
            for (index, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}[{index}]"), rows);
            }
        }
        (Value::Number(a), _) if a.is_f64() => {
            let expected_float = a.as_f64().expect("finite projected float");
            if actual.as_f64().map(f64::to_bits) != Some(expected_float.to_bits()) {
                rows.push(json!({"path":path,"expected":expected,"actual":actual,"expected_hex":float_hex(expected_float),"actual_hex":actual.as_f64().map(float_hex)}));
            }
        }
        _ if expected != actual => {
            rows.push(json!({"path":path,"expected":expected,"actual":actual}))
        }
        _ => {}
    }
}
pub fn differences(expected: &[Record], actual: &[Record]) -> Vec<Value> {
    let comparable = |records: &[Record]| {
        Value::Array(
            records
                .iter()
                .map(|row| json!({"service":row.service,"valid":row.valid,"data":row.data}))
                .collect(),
        )
    };
    let mut rows = Vec::new();
    compare(&comparable(expected), &comparable(actual), "", &mut rows);
    rows
}
