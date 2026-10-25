//! Source-derived restore and get_msg policy. Cereal supplies wire/schema only.
use crate::{lagd_frames, lagd_reference, seal::Result};
use capnp::{dynamic_value, message::ReaderOptions};
use openpilot_cereal::{
    car_capnp::car_params,
    log_capnp::{event, live_delay_data},
};
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::json;

fn scalar(value: impl std::fmt::Display) -> Result<JsonValue> {
    Ok(JsonValue::parse(&value.to_string())?)
}
fn normalized(value: f64) -> Result<JsonValue> {
    Ok(if value.is_nan() {
        JsonValue::text("nan")
    } else if value == f64::INFINITY {
        JsonValue::text("inf")
    } else if value == f64::NEG_INFINITY {
        JsonValue::text("-inf")
    } else {
        JsonValue::parse(&format!("{value:?}"))?
    })
}
pub(crate) fn object(rows: &[(&str, JsonValue)]) -> Result<JsonValue> {
    Ok(JsonValue::parse(&format!(
        "{{{}}}",
        rows.iter()
            .map(|(key, value)| Ok(format!(
                "{}:{}",
                serde_json::to_string(key)?,
                value.to_json()?
            )))
            .collect::<Result<Vec<_>>>()?
            .join(",")
    ))?)
}
/// Pycapnp's to_dict includes active unions, scalar defaults and present pointers.
fn project(value: dynamic_value::Reader<'_>) -> Result<JsonValue> {
    use dynamic_value::Reader as D;
    match value {
        D::Void => Ok(JsonValue::parse("null")?),
        D::Bool(v) => scalar(v),
        D::Int8(v) => scalar(v),
        D::Int16(v) => scalar(v),
        D::Int32(v) => scalar(v),
        D::Int64(v) => scalar(v),
        D::UInt8(v) => scalar(v),
        D::UInt16(v) => scalar(v),
        D::UInt32(v) => scalar(v),
        D::UInt64(v) => scalar(v),
        D::Float32(v) => normalized(f64::from(v)),
        D::Float64(v) => normalized(v),
        D::Text(v) => Ok(JsonValue::text(v.to_str()?)),
        D::Enum(v) => Ok(JsonValue::text(
            v.get_enumerant()?
                .ok_or("Member was null.")?
                .get_proto()
                .get_name()?
                .to_str()?,
        )),
        D::List(values) => lagd_frames::array(
            &(0..values.len())
                .map(|i| project(values.get(i)?))
                .collect::<Result<Vec<_>>>()?,
        ),
        D::Struct(value) => {
            let schema = value.get_schema();
            let mut fields = Vec::new();
            if let Some(field) = value.which()? {
                fields.push(field);
            } else if !schema.get_union_fields()?.is_empty() {
                return Err("unknown union discriminant".into());
            }
            let nonunion = schema.get_non_union_fields()?;
            fields.extend((0..nonunion.len()).map(|i| nonunion.get(i)));
            let mut rows = Vec::new();
            for field in fields {
                if value.has(field)? {
                    rows.push((
                        field.get_proto().get_name()?.to_str()?.to_owned(),
                        project(value.get(field)?)?,
                    ));
                }
            }
            object(
                &rows
                    .iter()
                    .map(|(key, value)| (key.as_str(), value.clone()))
                    .collect::<Vec<_>>(),
            )
        }
        D::Data(_) => Err("TypeError: bytes are not JSON serializable".into()),
        D::AnyPointer(_) | D::Capability(_) => Err("unsupported JSON schema pointer".into()),
    }
}
pub fn project_event(bytes: &[u8]) -> Result<JsonValue> {
    let message = capnp::serialize::read_message(&mut &bytes[..], ReaderOptions::new())?;
    project(message.get_root::<event::Reader<'_>>()?.into())
}
/// Match retrieve_initial_lag, including its deliberately absent lower bound and
/// acceptance of unknown enum values other than the named invalid status.
pub fn restore(car: &[u8], saved: Option<&[u8]>, previous: Option<&[u8]>) -> Result<JsonValue> {
    let car_message = capnp::serialize::read_message(&mut &car[..], ReaderOptions::new())?;
    let current = car_message.get_root::<car_params::Reader<'_>>()?;
    let mut remaining = saved.map(<[u8]>::len);
    let seed = if let Some(saved) = saved.filter(|value| !value.is_empty()) {
        let result = (|| -> Result<JsonValue> {
            let message = capnp::serialize::read_message(&mut &saved[..], ReaderOptions::new())?;
            let old_bytes = previous.ok_or("missing previous CarParams")?;
            let old_message =
                capnp::serialize::read_message(&mut &old_bytes[..], ReaderOptions::new())?;
            let event::LiveDelay(delay) = message.get_root::<event::Reader<'_>>()?.which()? else {
                return Err("cached Event is not liveDelay".into());
            };
            let delay = delay?;
            let old = old_message.get_root::<car_params::Reader<'_>>()?;
            if old.get_car_fingerprint()?.to_str()? != current.get_car_fingerprint()?.to_str()? {
                return Err("Car model mismatch".into());
            }
            let blocks = delay.get_valid_blocks();
            if blocks > 50 {
                return Err("Invalid number of valid blocks".into());
            }
            if delay.get_status() == Ok(live_delay_data::Status::Invalid) {
                return Err("Lag estimate is invalid".into());
            }
            lagd_frames::array(&[
                normalized(f64::from(delay.get_lateral_delay_estimate()))?,
                scalar(blocks)?,
            ])
        })();
        match result {
            Ok(seed) => seed,
            Err(_) => {
                remaining = None;
                JsonValue::parse("null")?
            }
        }
    } else {
        JsonValue::parse("null")?
    };
    object(&[
        ("seed", seed),
        (
            "remaining",
            JsonValue::parse(&json!(remaining).to_string())?,
        ),
    ])
}

pub(crate) struct PacketState<'a> {
    pub initial: f64,
    pub values: &'a [f64],
    pub block_idx: usize,
    pub idx: i64,
    pub valid_blocks: i64,
}
impl PacketState<'_> {
    pub(crate) fn statistics(&self) -> Result<[f64; 4]> {
        let mut valid = Vec::new();
        for index in 0..usize::try_from(self.valid_blocks.max(0))? {
            if index != self.block_idx {
                valid.push(*self.values.get(index).ok_or("block index out of bounds")?);
            }
        }
        fn stats(values: &[f64]) -> (f64, f64) {
            if values.is_empty() {
                return (f64::NAN, f64::NAN);
            }
            let mean = lagd_reference::numeric::sum(values) / values.len() as f64;
            let variance = lagd_reference::numeric::sum(
                &values
                    .iter()
                    .map(|value| (value - mean) * (value - mean))
                    .collect::<Vec<_>>(),
            ) / values.len() as f64;
            (mean, variance.sqrt())
        }
        let (mean, std) = stats(&valid);
        if self.idx > 0 {
            valid.push(
                *self
                    .values
                    .get(self.block_idx)
                    .ok_or("block index out of bounds")?,
            );
        }
        let (current, current_std) = stats(&valid);
        Ok([mean, std, current, current_std])
    }
    pub(crate) fn encode(&self, valid: bool, debug: bool) -> Result<Option<Vec<u8>>> {
        let [mean, std, current, current_std] = self.statistics()?;
        let status = if self.valid_blocks >= 5 && !mean.is_nan() && !std.is_nan() {
            if std > 0.1 {
                live_delay_data::Status::Invalid
            } else {
                live_delay_data::Status::Estimated
            }
        } else {
            live_delay_data::Status::Unestimated
        };
        // Python max/min preserve infinities; these branches exclude NaN first.
        let lateral = if status == live_delay_data::Status::Estimated {
            mean.clamp(0.15, 1.)
        } else {
            self.initial
        };
        let (estimate, estimate_std) = if !current.is_nan() && !current_std.is_nan() {
            (current, current_std)
        } else {
            (self.initial, 0.)
        };
        let progress = ((i128::from(self.valid_blocks) * 100 + i128::from(self.idx)) * 100)
            .div_euclid(500)
            .min(100);
        let (Ok(blocks), Ok(progress)) = (i32::try_from(self.valid_blocks), i8::try_from(progress))
        else {
            return Ok(None);
        };
        let mut message = capnp::message::Builder::new_default();
        let mut root = message.init_root::<event::Builder<'_>>();
        root.set_log_mono_time(123456789);
        root.set_valid(valid);
        let mut delay = root.init_live_delay();
        delay.set_status(status);
        delay.set_lateral_delay(lateral as f32);
        delay.set_lateral_delay_estimate(estimate as f32);
        delay.set_lateral_delay_estimate_std(estimate_std as f32);
        delay.set_valid_blocks(blocks);
        delay.set_cal_perc(progress);
        if debug {
            let mut points = delay.init_points(self.values.len().try_into()?);
            for (i, value) in self.values.iter().enumerate() {
                points.set(i as u32, *value as f32);
            }
        }
        Ok(Some(capnp::serialize::write_message_to_words(&message)))
    }
}
fn flatten(value: &JsonValue, output: &mut Vec<f64>) -> Result<()> {
    if let JsonView::Array(values) = value.view() {
        for value in values {
            flatten(&value, output)?;
        }
    } else {
        output.push(lagd_frames::number(value)?);
    }
    Ok(())
}
pub fn packet(input: &JsonValue) -> Result<Option<Vec<u8>>> {
    let blocks = lagd_frames::integer(&lagd_frames::field(input, "blocks")?)?;
    let idx = lagd_frames::integer(&lagd_frames::field(input, "idx")?)?;
    let mut values = Vec::new();
    flatten(&lagd_frames::field(input, "values")?, &mut values)?;
    let flag = |key| -> Result<bool> {
        match lagd_frames::field(input, key)?.view() {
            JsonView::Bool(v) => Ok(v),
            _ => Err("packet flag must be bool".into()),
        }
    };
    PacketState {
        initial: 0.4,
        values: &values,
        block_idx: usize::try_from(blocks.rem_euclid(50))?,
        idx,
        valid_blocks: blocks,
    }
    .encode(flag("valid")?, flag("debug")?)
}
