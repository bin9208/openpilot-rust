use super::{fixture, numeric, require, Result};
use capnp::{message::ReaderOptions, serialize};
use openpilot_cereal::log_capnp::event;
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};
pub const READERS: [&str; 16] = [
    "selfdriveState",
    "carState",
    "liveParameters",
    "modelV2",
    "liveCalibration",
    "livePose",
    "longitudinalPlan",
    "carOutput",
    "carrotMan",
    "lateralPlan",
    "radarState",
    "driverMonitoringState",
    "onroadEvents",
    "driverAssistance",
    "liveDelay",
    "liveTorqueParameters",
];
pub fn reader_header(header: &[u8], pid: u32) -> Result<bool> {
    require(header.len() == 984, "short msgq reader header")?;
    let word =
        |i: usize| u64::from_le_bytes(header[i * 8..i * 8 + 8].try_into().expect("header range"));
    Ok((0..word(0).min(40) as usize).any(|i| word(43 + i) != 0 && word(83 + i) as u32 == pid))
}
pub fn writer_header(header: &[u8], pid: u32) -> Result<bool> {
    require(header.len() == 24, "short msgq writer header")?;
    Ok(u64::from_le_bytes(header[16..24].try_into()?) as u32 == pid)
}
pub fn persisted(values: &BTreeMap<String, String>) -> Result<()> {
    require(
        *values
            == BTreeMap::from([
                ("LongitudinalPersonalityMax".into(), "3".into()),
                ("HyundaiCameraSccHint".into(), "0".into()),
            ]),
        "persisted Params differ from original 3/0 contract",
    )
}
pub struct Pair {
    pub controls_time: u64,
    pub control_time: u64,
    pub has_input: bool,
    pub values: Value,
}
pub fn pair(packets: &[Vec<u8>]) -> Result<Pair> {
    require(
        packets.len() == 2,
        "publication pair must have exactly two packets",
    )?;
    let mut stamps = Vec::new();
    let mut has_input = false;
    let mut values = Vec::new();
    for (index, (bytes, service)) in packets
        .iter()
        .zip(["controlsState", "carControl"])
        .enumerate()
    {
        require(
            fixture::service(bytes)? == service,
            "publication service order differs",
        )?;
        let mut input = bytes.as_slice();
        let message = serialize::read_message(&mut input, ReaderOptions::new())?;
        require(input.is_empty(), "trailing publication bytes")?;
        let event = message.get_root::<event::Reader<'_>>()?;
        stamps.push(event.get_log_mono_time());
        if index == 0 {
            if let event::ControlsState(state) = event.which()? {
                has_input = state?.get_lateral_plan_mono_time() != 0;
            } else {
                return Err("controlsState union required".into());
            }
        }
        let projected = fixture::project_event(bytes)?.to_json()?;
        values
            .push(json!({"service":service,"event":numeric::parse_python(projected.as_bytes())?}));
    }
    require(
        stamps[0] <= stamps[1],
        "controlsState clock exceeds carControl clock",
    )?;
    Ok(Pair {
        controls_time: stamps[0],
        control_time: stamps[1],
        has_input,
        values: json!(values),
    })
}
pub fn compare_rows(original: &Value, packets: &[Vec<Vec<u8>>]) -> Result<numeric::Stats> {
    let rows = original["rows"]
        .as_array()
        .ok_or("original rows required")?;
    require(
        rows.len() == 140 && packets.len() == 140,
        "exactly 140 source/native rows required",
    )?;
    let mut stats = numeric::Stats::default();
    for (source, raw) in rows.iter().zip(packets) {
        let pair = pair(raw)?;
        let expected = source["publications"]
            .as_array()
            .ok_or("original publications required")?;
        require(
            expected.len() == 2,
            "exactly two original publications required",
        )?;
        for (left, right) in expected
            .iter()
            .zip(pair.values.as_array().ok_or("decoded pair")?)
        {
            let (mut left, mut right) = (left.clone(), right.clone());
            for value in [&mut left, &mut right] {
                require(
                    value["event"]
                        .as_object_mut()
                        .ok_or("Event object required")?
                        .remove("logMonoTime")
                        .is_some(),
                    "missing top-level publication clock",
                )?;
            }
            numeric::compare(&left, &right, "native.event", &mut stats)?;
        }
    }
    Ok(stats)
}
pub fn observations(
    input: &Value,
    packets: &[Vec<Vec<u8>>],
    facts: &Value,
    signal: i32,
    daemon: &Path,
) -> Result<()> {
    require(
        facts["capture_complete"] == true
            && facts["signal"] == signal
            && facts["exit"] == 0
            && facts["frames"] == 140
            && facts["publications"] == 280
            && facts["restart_from_persisted_params"] == (signal == 15)
            && facts["startup_waits_for_carparams"] == true
            && facts["no_python_runtime"] == true,
        "native phase facts differ",
    )?;
    let shutdown = facts["shutdown_seconds"]
        .as_f64()
        .ok_or("shutdown duration required")?;
    require(
        shutdown.is_finite() && (0.0..=5.0).contains(&shutdown),
        "shutdown exceeds five seconds",
    )?;
    let close = facts["native_ipc_close_seconds"]
        .as_f64()
        .ok_or("IPC close duration required")?;
    require(
        close.is_finite() && (0.0..=5.0).contains(&close),
        "native IPC peer close exceeds five seconds",
    )?;
    require(
        facts["native_executable"] == json!(daemon),
        "phase native executable differs",
    )?;
    require(
        input["name"] == format!("native-signal-{signal}") && input["simulation"] == true,
        "source case/context differs",
    )?;
    let frames = input["frames"].as_array().ok_or("source frames required")?;
    require(
        packets.len() == 140 && frames.len() == 140,
        "frame count differs",
    )?;
    let row = super::capture::fixture_row()?;
    let messages = row["messages"].clone();
    let mut received = false;
    for (frame, raw) in frames.iter().zip(packets) {
        let value = pair(raw)?;
        require(
            frame["time"].as_f64() == Some(value.controls_time as f64 / 1e9),
            "source frame clock differs",
        )?;
        require(
            frame["messages"]
                == if value.has_input && !received {
                    messages.clone()
                } else {
                    json!([])
                },
            "source frame attribution differs",
        )?;
        received |= value.has_input;
    }
    require(received, "no published input was observed")
}
