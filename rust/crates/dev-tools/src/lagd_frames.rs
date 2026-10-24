//! Independent translation of lagd_frames.py, using only cereal wire mechanics.
use crate::seal::Result;
use capnp::{dynamic_value, introspect::TypeVariant, message::ReaderOptions};
use openpilot_cereal::{
    car_capnp::car_params,
    log_capnp::{event, live_calibration_data},
};
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::json;

pub const TOPICS: [&str; 5] = [
    "livePose",
    "liveCalibration",
    "carState",
    "controlsState",
    "carControl",
];
pub(crate) fn field(row: &JsonValue, key: &str) -> Result<JsonValue> {
    row.get(key)
        .ok_or_else(|| format!("KeyError: {key}").into())
}
pub(crate) fn number(row: &JsonValue) -> Result<f64> {
    match row.view() {
        JsonView::Float(value) => Ok(value),
        JsonView::Bool(value) => Ok(f64::from(u8::from(value))),
        JsonView::Integer(value) => {
            let number: f64 = value.parse()?;
            if !number.is_finite() {
                return Err("int too large to convert to float".into());
            }
            Ok(number)
        }
        _ => Err("TypeError: cereal numeric field".into()),
    }
}
pub(crate) fn integer(row: &JsonValue) -> Result<i64> {
    match row.view() {
        JsonView::Integer(value) => Ok(value.parse()?),
        JsonView::Bool(value) => Ok(i64::from(value)),
        _ => Err("TypeError: integer required".into()),
    }
}
pub(crate) fn bytes(row: &JsonValue) -> Result<Vec<u8>> {
    // The source explicitly calls bytes(value), which also admits a scalar
    // nonnegative integer (zero-filled length) and an empty mapping iterator.
    match row.view() {
        JsonView::Array(rows) => rows
            .iter()
            .map(|row| Ok(u8::try_from(integer(row)?)?))
            .collect(),
        JsonView::Object(rows) if rows.is_empty() => Ok(Vec::new()),
        JsonView::Integer(_) | JsonView::Bool(_) => {
            let length = match row.view() {
                JsonView::Integer(value) => value.parse::<usize>()?,
                JsonView::Bool(value) => usize::from(value),
                _ => unreachable!(),
            };
            let mut bytes = Vec::new();
            bytes.try_reserve_exact(length)?;
            bytes.resize(length, 0);
            Ok(bytes)
        }
        _ => Err("TypeError: expected byte sequence".into()),
    }
}
pub(crate) fn optional_bytes(row: &JsonValue) -> Result<Option<Vec<u8>>> {
    if matches!(row.view(), JsonView::Null) {
        Ok(None)
    } else {
        Ok(Some(bytes(row)?))
    }
}
pub(crate) fn json_bytes(bytes: &[u8]) -> Result<JsonValue> {
    Ok(JsonValue::parse(&serde_json::to_string(bytes)?)?)
}
pub(crate) fn array(rows: &[JsonValue]) -> Result<JsonValue> {
    Ok(JsonValue::parse(&format!(
        "[{}]",
        rows.iter()
            .map(JsonValue::to_json)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join(",")
    ))?)
}
pub(crate) fn set_enum<'a>(
    value: impl Into<dynamic_value::Builder<'a>>,
    field: &str,
    raw: u16,
) -> Result<()> {
    let dynamic_value::Builder::Struct(mut builder) = value.into() else {
        return Err("enum owner is not a struct".into());
    };
    let slot = builder.get_schema().get_field_by_name(field)?;
    let TypeVariant::Enum(schema) = slot.get_type().which() else {
        return Err("enum field schema changed".into());
    };
    builder.set(slot, dynamic_value::Enum::new(raw, schema.into()).into())?;
    Ok(())
}
pub fn car_params(name: &str, delay: f64) -> Result<Vec<u8>> {
    let mut message = capnp::message::Builder::new_default();
    let mut car = message.init_root::<car_params::Builder<'_>>();
    car.set_car_fingerprint(name);
    car.set_steer_actuator_delay(delay as f32);
    Ok(capnp::serialize::write_message_to_words(&message))
}
pub fn cached(lag: f64, blocks: i64, status: &JsonValue) -> Result<Vec<u8>> {
    let mut message = capnp::message::Builder::new_default();
    let mut delay = message.init_root::<event::Builder<'_>>().init_live_delay();
    delay.set_lateral_delay_estimate(lag as f32);
    delay.set_valid_blocks(i32::try_from(blocks)?);
    let raw = match status.view() {
        JsonView::Text(_) => match status.to_utf8().ok_or("invalid enum text")?.as_str() {
            "unestimated" => 0,
            "estimated" => 1,
            "invalid" => 2,
            _ => return Err("unknown LiveDelay status".into()),
        },
        _ => u16::try_from(integer(status)?)?,
    };
    set_enum(delay, "status", raw)?;
    Ok(capnp::serialize::write_message_to_words(&message))
}
pub fn messages(index: i64, mode: &str) -> Result<Vec<Vec<u8>>> {
    let time = index as f64 * 0.05;
    let desired = 0.3 * (time * 5.).sin() + 0.02 * (time * 11.).sin();
    let actual = 0.3 * ((time - 0.35) * 5.).sin() + 0.02 * ((time - 0.35) * 11.).sin();
    let stamp = ((100. + time) * 1e9).round_ties_even();
    if !(0. ..18_446_744_073_709_551_616.).contains(&stamp) {
        return Err("logMonoTime outside UInt64".into());
    }
    let recovering = |start, end| mode == "recovery" && index >= start && index < end;
    let mut output = Vec::with_capacity(5);
    for topic in TOPICS {
        let mut message = capnp::message::Builder::new_default();
        let mut root = message.init_root::<event::Builder<'_>>();
        root.set_log_mono_time(stamp as u64);
        root.set_valid(true);
        match topic {
            "carControl" => root
                .init_car_control()
                .set_lat_active(!recovering(300, 310)),
            "carState" => {
                let mut value = root.init_car_state();
                value.set_v_ego(if recovering(500, 505) { 15. } else { 20. });
                value.set_steering_pressed(recovering(700, 710));
            }
            "controlsState" => {
                let mut value = root.init_controls_state();
                value.set_desired_curvature((desired / 20. / 20.) as f32);
                let control = value.init_lateral_control_state();
                let saturated = recovering(900, 910);
                match index.rem_euclid(4) {
                    0 => control.init_pid_state().set_saturated(saturated),
                    1 => control.init_angle_state().set_saturated(saturated),
                    2 => control.init_debug_state().set_saturated(saturated),
                    _ => control.init_torque_state().set_saturated(saturated),
                }
            }
            "liveCalibration" => {
                let mut value = root.init_live_calibration();
                let mut rpy = value.reborrow().init_rpy_calib(3);
                for i in 0..3 {
                    rpy.set(i, 0.);
                }
                value.set_cal_status(if recovering(1100, 1110) {
                    live_calibration_data::Status::Uncalibrated
                } else {
                    live_calibration_data::Status::Calibrated
                });
            }
            "livePose" => {
                let mut value = root.reborrow().init_live_pose();
                let mut angular = value.reborrow().init_angular_velocity_device();
                angular.set_z((actual / 20.) as f32);
                angular.set_z_std(0.01);
                angular.set_valid(!recovering(1150, 1160));
                value.set_posenet_o_k(true);
                value.set_inputs_o_k(true);
                value.reborrow().init_orientation_n_e_d().set_valid(false);
                value.set_sensors_o_k(false);
                if recovering(1200, 1210) {
                    root.set_valid(false);
                }
            }
            _ => unreachable!(),
        }
        output.push(capnp::serialize::write_message_to_words(&message));
    }
    Ok(output)
}
/// Match source as_builder()/to_bytes() after changing only logMonoTime.
pub fn rewrite_timestamp(bytes: &[u8], delta: u64) -> Result<Vec<u8>> {
    let reader = capnp::serialize::read_message(&mut &bytes[..], ReaderOptions::new())?;
    let event = reader.get_root::<event::Reader<'_>>()?;
    let stamp = event
        .get_log_mono_time()
        .checked_add(delta)
        .ok_or("timestamp overflow")?;
    let mut message = capnp::message::Builder::new_default();
    message.set_root(event)?;
    message
        .get_root::<event::Builder<'_>>()?
        .set_log_mono_time(stamp);
    Ok(capnp::serialize::write_message_to_words(&message))
}
/// Lossless argument adapter for the three original helper constructors.
pub fn generate(input: &JsonValue) -> Result<JsonValue> {
    let kind = field(input, "kind")?.to_utf8().ok_or("kind must be text")?;
    match kind.as_str() {
        "car" => json_bytes(&car_params(
            &field(input, "name")?
                .to_utf8()
                .ok_or("car name must be text")?,
            number(&field(input, "delay")?)?,
        )?),
        "cached" => json_bytes(&cached(
            number(&field(input, "lag")?)?,
            integer(&field(input, "blocks")?)?,
            &field(input, "status")?,
        )?),
        "messages" => {
            let rows = messages(
                integer(&field(input, "index")?)?,
                &field(input, "mode")?.to_utf8().ok_or("mode must be text")?,
            )?;
            Ok(JsonValue::parse(&json!(rows).to_string())?)
        }
        _ => Err("unknown lagd frame constructor".into()),
    }
}
