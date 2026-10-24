use super::numeric::{self, Stats};
use capnp::dynamic_value::Reader as Dynamic;
use openpilot_cereal::log_capnp::event;
use openpilot_dev_tools::seal::Result;
use serde_json::{json, Map, Value};

pub const NAMES: [&str; 7] = [
    "physical-model-0",
    "physical-model-1",
    "physical-model-2",
    "active-fallback-and-thresholds",
    "clipping-hysteresis-and-float32-validity",
    "nonfinite-state-keeps-prereset-std",
    "rewind-eviction-and-inactive-clock-reset",
];
pub const COUNTS: [usize; 7] = [321, 321, 321, 29, 64, 4, 136];

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
        Dynamic::Float32(v) => numeric::float(f64::from(v)),
        Dynamic::Float64(v) => numeric::float(v),
        Dynamic::Text(v) => json!(v.to_str()?),
        Dynamic::Enum(v) => json!(v
            .get_enumerant()?
            .ok_or("unknown Event enum")?
            .get_proto()
            .get_name()?
            .to_str()?),
        Dynamic::List(v) => Value::Array(v.iter().map(|x| project(x?)).collect::<Result<_>>()?),
        Dynamic::Struct(v) => {
            let mut fields = Map::new();
            if let Some(field) = v.which()? {
                fields.insert(
                    field.get_proto().get_name()?.to_str()?.into(),
                    project(v.get(field)?)?,
                );
            } else if !v.get_schema().get_union_fields()?.is_empty() {
                return Err("unknown Event union".into());
            }
            for field in v.get_schema().get_non_union_fields()? {
                if v.has(field)? {
                    fields.insert(
                        field.get_proto().get_name()?.to_str()?.into(),
                        project(v.get(field)?)?,
                    );
                }
            }
            Value::Object(fields)
        }
        Dynamic::Data(_) | Dynamic::AnyPointer(_) | Dynamic::Capability(_) => {
            return Err("unsupported Event pointer".into())
        }
    })
}
pub fn event_dict(bytes: &[u8]) -> Result<Value> {
    let mut input = bytes;
    let message = capnp::serialize::read_message(&mut input, capnp::message::ReaderOptions::new())?;
    if !input.is_empty() {
        return Err("trailing estimator Event bytes".into());
    }
    // No dropped timestamp, debug field, log, validity or schema projection.
    project(message.get_root::<event::Reader<'_>>()?.into())
}
fn floats(bits: Value, count: usize) -> Result<Value> {
    let bits = bits.as_array().ok_or("estimator float-bit list")?;
    if bits.len() != count {
        return Err("estimator float-bit dimension changed".into());
    }
    Ok(Value::Array(
        bits.iter()
            .map(|v| {
                v.as_u64()
                    .map(|x| numeric::float(f64::from_bits(x)))
                    .ok_or_else(|| "estimator float-bit value".into())
            })
            .collect::<Result<_>>()?,
    ))
}
pub fn validate_roster(value: &Value, field: &str) -> Result<()> {
    let cases = value.as_array().ok_or("estimator cases must be a list")?;
    if cases.len() != 7 {
        return Err("estimator must contain seven cases".into());
    }
    for (index, case) in cases.iter().enumerate() {
        if case["name"] != NAMES[index]
            || case[field].as_array().map(Vec::len) != Some(COUNTS[index])
        {
            return Err(format!("estimator roster/count differs at case{index}").into());
        }
    }
    Ok(())
}
pub fn normalize(raw: &mut Value) -> Result<()> {
    validate_roster(raw, "rows")?;
    for case in raw.as_array_mut().ok_or("native cases list")? {
        for row in case["rows"].as_array_mut().ok_or("native rows list")? {
            let fields = row.as_object_mut().ok_or("native row object")?;
            for (raw_key, key, count) in [
                ("x_bits", "x", 9),
                ("p_bits", "p", 81),
                ("observed_bits", "observed", 3),
            ] {
                if fields.contains_key(key) {
                    return Err("native raw row already has decoded field".into());
                }
                let bits = fields
                    .remove(raw_key)
                    .ok_or("missing raw estimator float bits")?;
                fields.insert(key.into(), floats(bits, count)?);
            }
            if fields.contains_key("event") {
                return Err("native raw row already has decoded Event".into());
            }
            let packet: Vec<u8> = serde_json::from_value(
                fields
                    .remove("packet")
                    .ok_or("missing raw estimator Event")?,
            )?;
            fields.insert("event".into(), event_dict(&packet)?);
        }
    }
    Ok(())
}
#[derive(Debug, serde::Serialize)]
pub struct Summary {
    pub pass: bool,
    pub cases: usize,
    pub steps: usize,
    #[serde(flatten)]
    pub numbers: Stats,
    pub float64_tolerance: Value,
    pub float32_tolerance: Value,
}
pub fn compare(source: &Value, raw: &Value) -> Result<Summary> {
    validate_roster(source, "rows")?;
    let mut actual = raw.clone();
    normalize(&mut actual)?;
    let mut numbers = Stats::default();
    numeric::compare(source, &actual, "paramsd", &mut numbers)?;
    Ok(Summary {
        pass: true,
        cases: 7,
        steps: COUNTS.iter().sum(),
        numbers,
        float64_tolerance: json!({"absolute":1e-9,"relative":1e-9}),
        float32_tolerance: json!({"absolute":2e-6,"relative":2e-6}),
    })
}
