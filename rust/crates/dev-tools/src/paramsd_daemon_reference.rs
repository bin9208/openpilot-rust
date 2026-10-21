//! Sealed original-only numeric oracle for the paramsd daemon controller.
//! Call after both native phases and before publishing the top-level receipt.
use bzip2::read::BzDecoder;
use capnp::{dynamic_struct, dynamic_value::Reader as Dynamic};
use openpilot_cereal::log_capnp::event;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Cursor, Read},
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
struct Lane {
    name: &'static str,
    count: usize,
    source: &'static [u8],
    source_sha: &'static str,
    packed_sha: &'static str,
    result: &'static [u8],
    result_sha: &'static str,
    gps: &'static [u8],
    gps_sha: &'static str,
    saved: &'static [u8],
    saved_sha: &'static str,
    input: &'static [u8],
    input_sha: &'static str,
    input_packed_sha: &'static str,
}

const LANES: [Lane; 2] = [
    Lane {
        name: "continuous",
        count: 1202,
        source: include_bytes!(
            "../tests/fixtures/paramsd-daemon-original/continuous-source.json.bz2"
        ),
        source_sha: "5b20c0a259475fcd4440139cd955d2b68961f690a92827bd9e37b23b19bc8940",
        packed_sha: "087818fb26f4db9fdd8b5ea5c1bfd5813c934c0af88c8035061fdf761c92ef90",
        result: include_bytes!("../tests/fixtures/paramsd-daemon-original/continuous-results.json"),
        result_sha: "3d197558159b3a1aea767ff06189bba6e20e73112ba5bec871f98062017f8c0d",
        gps: include_bytes!("../tests/fixtures/paramsd-daemon-original/continuous-gps.json"),
        gps_sha: "40dc50f8d21d5e67b72e7071306bdc622621919295e68675cbe6a921ce6dbcaa",
        saved: include_bytes!("../tests/fixtures/paramsd-daemon-original/continuous-saved.bin"),
        saved_sha: "3cde332d4f7efa9596df27e307fe49f0fa7dcd3e7401a6657ef9962e1e5c63b0",
        input: include_bytes!(
            "../tests/fixtures/paramsd-daemon-original/continuous-input.json.bz2"
        ),
        input_sha: "1aff9d74644834accfdd773233b98afc8c4304b40c704c91d09de875225c48ea",
        input_packed_sha: "b90e04018c0a147c1b92c2bb14aca83b9712ffd2184309678f2f5c0f07b9b620",
    },
    Lane {
        name: "restart",
        count: 8,
        source: include_bytes!("../tests/fixtures/paramsd-daemon-original/restart-source.json.bz2"),
        source_sha: "ce7cd621f0ac5ce7ce04add128d542901cea35ca36b7c322b506584df2d439ee",
        packed_sha: "91bb4c2a9518edcbc242dd3490f156de766ae6485d0bf946564a279ad185ee2b",
        result: include_bytes!("../tests/fixtures/paramsd-daemon-original/restart-results.json"),
        result_sha: "433c303b8c387c62dee4b285420d83a8c3cbbaa90c7db3d29fa85fc0f798c7f6",
        gps: include_bytes!("../tests/fixtures/paramsd-daemon-original/restart-gps.json"),
        gps_sha: "40dc50f8d21d5e67b72e7071306bdc622621919295e68675cbe6a921ce6dbcaa",
        saved: include_bytes!("../tests/fixtures/paramsd-daemon-original/restart-saved.bin"),
        saved_sha: "3cde332d4f7efa9596df27e307fe49f0fa7dcd3e7401a6657ef9962e1e5c63b0",
        input: include_bytes!("../tests/fixtures/paramsd-daemon-original/restart-input.json.bz2"),
        input_sha: "6755642041f21f4e1be89bedc4b07cab99357ae516ed571bb23d4477fa82867f",
        input_packed_sha: "8a46bedc0d9c24b5906a64e10370899efabaceb4094bd91da8f4870a8886cc61",
    },
];

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn require_equal<T: PartialEq + std::fmt::Debug>(left: T, right: T, path: &str) -> Result<()> {
    if left != right {
        return Err(format!("{path}: {left:?} != {right:?}").into());
    }
    Ok(())
}
fn unpack(packed: &[u8], packed_sha: &str, raw_sha: &str, label: &str) -> Result<Value> {
    require_equal(sha(packed), packed_sha.to_string(), label)?;
    let mut raw = Vec::new();
    BzDecoder::new(packed)
        .take(8_000_001)
        .read_to_end(&mut raw)?;
    if raw.len() > 8_000_000 {
        return Err(format!("{label}: original fixture exceeds bounded size").into());
    }
    require_equal(sha(&raw), raw_sha.to_string(), label)?;
    Ok(serde_json::from_slice(&raw)?)
}
fn source(lane: &Lane) -> Result<Value> {
    unpack(
        lane.source,
        lane.packed_sha,
        lane.source_sha,
        "original source",
    )
}
fn original_input(lane: &Lane) -> Result<Value> {
    unpack(
        lane.input,
        lane.input_packed_sha,
        lane.input_sha,
        "original input",
    )
}

// pycapnp Event.to_dict() projects the active union, defaults, and present
// pointer fields. This reader neither calls the native controller nor trusts
// its own packet interpretation.
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
        Dynamic::Float32(v) => json!(f64::from(v)),
        Dynamic::Float64(v) => json!(v),
        Dynamic::Text(v) => json!(v.to_str()?),
        Dynamic::Enum(v) => json!(v
            .get_enumerant()?
            .ok_or("unknown cereal enum")?
            .get_proto()
            .get_name()?
            .to_str()?),
        Dynamic::List(v) => {
            Value::Array(v.iter().map(|item| project(item?)).collect::<Result<_>>()?)
        }
        Dynamic::Struct(v) => Value::Object(structure(v)?),
        Dynamic::Data(_) | Dynamic::AnyPointer(_) | Dynamic::Capability(_) => {
            return Err("unsupported Cereal Event pointer".into())
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
        return Err("unknown Cereal union discriminant".into());
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
fn event_dict(bytes: &[u8]) -> Result<Value> {
    let message = capnp::serialize::read_message(
        &mut Cursor::new(bytes),
        capnp::message::ReaderOptions::new(),
    )?;
    let mut value = project(message.get_root::<event::Reader<'_>>()?.into())?;
    value
        .as_object_mut()
        .ok_or("Event not an object")?
        .remove("logMonoTime");
    Ok(value)
}

// Same 2e-6 Event scalar / 1e-9 non-Event and debugFilterState policy as
// check_paramsd.compare. All structure, integer, string, and array mismatches
// are exact failures; no native values are used to construct expectations.
fn compare(expected: &Value, actual: &Value, path: &str) -> Result<()> {
    match expected {
        Value::Object(fields) => {
            let found = actual
                .as_object()
                .ok_or_else(|| format!("{path}: expected object"))?;
            require_equal(
                fields.keys().collect::<Vec<_>>(),
                found.keys().collect::<Vec<_>>(),
                path,
            )?;
            for (key, value) in fields {
                compare(value, &found[key], &format!("{path}.{key}"))?;
            }
        }
        Value::Array(items) => {
            let found = actual
                .as_array()
                .ok_or_else(|| format!("{path}: expected array"))?;
            require_equal(items.len(), found.len(), path)?;
            for (index, (left, right)) in items.iter().zip(found).enumerate() {
                compare(left, right, &format!("{path}[{index}]"))?;
            }
        }
        Value::Number(number) if number.is_f64() => {
            let left = number.as_f64().ok_or("original float")?;
            let right = actual
                .as_f64()
                .ok_or_else(|| format!("{path}: expected number"))?;
            let tolerance = if path.contains(".event.") && !path.contains(".debugFilterState.") {
                2e-6
            } else {
                1e-9
            };
            if !(left.is_finite()
                && right.is_finite()
                && (left - right).abs() <= tolerance + tolerance * right.abs())
            {
                return Err(format!("{path}: {left} != {right}").into());
            }
        }
        _ => require_equal(expected, actual, path)?,
    }
    Ok(())
}

fn read(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn normalized_restart_seed(bytes: &[u8]) -> Result<Vec<u8>> {
    // The independently captured seed is a 272-byte, one-segment Cap'n Proto
    // message. Validate its framing and root struct pointer before locating
    // Event.logMonoTime in the first root data word. No other byte is masked.
    if bytes.len() != 272 {
        return Err("restart seed has unexpected Cap'n Proto length".into());
    }
    let segment_count_minus_one = u32::from_le_bytes(bytes[0..4].try_into()?);
    let segment_words = u32::from_le_bytes(bytes[4..8].try_into()?);
    if segment_count_minus_one != 0
        || segment_words != 33
        || bytes.len() != 8 + segment_words as usize * 8
    {
        return Err("restart seed has unsupported Cap'n Proto framing".into());
    }
    let pointer = u64::from_le_bytes(bytes[8..16].try_into()?);
    let kind = pointer & 3;
    let offset = (pointer >> 2) & 0x3fff_ffff;
    let data_words = (pointer >> 32) & 0xffff;
    let pointer_words = pointer >> 48;
    if kind != 0 || offset != 0 || data_words != 2 || pointer_words != 1 {
        return Err("restart seed Event root layout changed".into());
    }
    let timestamp = 8 + 8; // segment header + root pointer + zero target offset
    let message = capnp::serialize::read_message(
        &mut Cursor::new(bytes),
        capnp::message::ReaderOptions::new(),
    )?;
    let root = message.get_root::<event::Reader<'_>>()?;
    if !matches!(root.which()?, event::LiveParameters(_))
        || root.get_log_mono_time()
            != u64::from_le_bytes(bytes[timestamp..timestamp + 8].try_into()?)
    {
        return Err("restart seed is not the expected Event layout".into());
    }
    let mut normalized = bytes.to_vec();
    normalized[timestamp..timestamp + 8].fill(0);
    Ok(normalized)
}

fn compare_input(lane: &Lane, current_path: &Path) -> Result<()> {
    let mut expected = original_input(lane)?;
    let mut current = read(current_path)?;
    if lane.name == "restart" {
        // The restart seed comes from this run's original continuous phase.
        // Its event timestamp varies, while every other parsed case value,
        // including ordered raw input packets, must match exactly.
        let take_saved = |value: &mut Value| -> Result<Vec<u8>> {
            let seed = value["seed"].as_object_mut().ok_or("restart seed object")?;
            let packet = seed
                .get_mut("LiveParametersV2")
                .ok_or("restart saved seed")?;
            Ok(serde_json::from_value(std::mem::take(packet))?)
        };
        let original_saved = take_saved(&mut expected)?;
        let current_saved = take_saved(&mut current)?;
        require_equal(
            normalized_restart_seed(&original_saved)?,
            normalized_restart_seed(&current_saved)?,
            "restart seed except Event.logMonoTime",
        )?;
    }
    if expected != current {
        return Err(format!("{} full input differs from sealed original", lane.name).into());
    }
    Ok(())
}

fn compare_phase_result(lane: &Lane, original: &Value, native: &Value) -> Result<()> {
    require_equal(original["pass"].clone(), json!(true), "original result")?;
    require_equal(
        original["source_equal"].clone(),
        json!(true),
        "original source comparison",
    )?;
    for (key, want) in [
        ("publications", json!(lane.count)),
        ("gps_service", original["gps_service"].clone()),
        ("signal", original["signal"].clone()),
        ("exit", original["exit"].clone()),
        ("startup_poll", original["startup_poll"].clone()),
    ] {
        require_equal(native[key].clone(), want, &format!("{}:{key}", lane.name))?;
    }
    require_equal(native["pass"].clone(), json!(true), "phase pass")?;
    require_equal(
        native["no_python_runtime"].clone(),
        json!(true),
        "Python absence",
    )?;
    let seconds = native["shutdown_seconds"]
        .as_f64()
        .ok_or("shutdown seconds")?;
    if !seconds.is_finite() || !(0.0..=5.0).contains(&seconds) {
        return Err(format!("{}: unbounded shutdown", lane.name).into());
    }
    if lane.name == "continuous" {
        require_equal(
            native["persisted_packet_index"].clone(),
            json!(1199),
            "saved packet index",
        )?;
    } else {
        require_equal(
            native["persisted_packet_index"].clone(),
            json!(null),
            "restart saved packet index",
        )?;
    }
    Ok(())
}
fn compare_gps(lane: &Lane, actual: &[u8]) -> Result<()> {
    require_equal(actual, lane.gps, "original GPS bytes")
}
fn compare_saved(lane: &Lane, actual: &[u8]) -> Result<()> {
    compare(
        &event_dict(lane.saved)?,
        &event_dict(actual)?,
        &format!("{}.event.saved", lane.name),
    )
}

fn compare_lane(lane: &Lane, input: &Path, phase: &Path, report: &Value) -> Result<()> {
    let original = source(lane)?;
    compare_input(lane, input)?;
    for (bytes, expected, label) in [
        (lane.result, lane.result_sha, "result"),
        (lane.gps, lane.gps_sha, "GPS"),
        (lane.saved, lane.saved_sha, "saved"),
    ] {
        require_equal(sha(bytes), expected.to_string(), label)?;
    }
    let original_result: Value = serde_json::from_slice(lane.result)?;
    compare_phase_result(lane, &original_result, report)?;
    compare_gps(lane, &fs::read(phase.join("gps.json"))?)?;
    compare_saved(lane, &fs::read(phase.join("saved.bin"))?)?;
    let rows = original["rows"].as_array().ok_or("original rows")?;
    require_equal(rows.len(), lane.count + 1, "original row count")?;
    let packets = read(&phase.join("packets.json"))?;
    let packets = packets.as_array().ok_or("native packets array")?;
    require_equal(packets.len(), lane.count, "native packet count")?;
    let journal = fs::read_to_string(phase.join("packets.jsonl"))?;
    let journal = journal.lines().collect::<Vec<_>>();
    require_equal(journal.len(), lane.count, "native packet journal count")?;
    for (index, (reference, packet)) in rows[1..].iter().zip(packets).enumerate() {
        let raw: Vec<u8> = serde_json::from_value(packet.clone())?;
        let journal_packet: Value = serde_json::from_str(journal[index])?;
        require_equal(
            &journal_packet,
            packet,
            &format!("{} packet {index} journal", lane.name),
        )?;
        let mut expected = reference["event"].clone();
        expected
            .as_object_mut()
            .ok_or("original event object")?
            .remove("logMonoTime");
        compare(
            &expected,
            &event_dict(&raw)?,
            &format!("{}.event.row{index}", lane.name),
        )?;
    }
    Ok(())
}

/// Check one completed native phase before its `pass:true` result.json is
/// written. Only the sealed original and already-persisted phase evidence are
/// read; a failure leaves the phase without a successful receipt.
pub fn compare_phase_before_receipt(
    name: &str,
    input: &Path,
    phase: &Path,
    report: &Value,
) -> Result<()> {
    let lane = LANES
        .iter()
        .find(|lane| lane.name == name)
        .ok_or("unknown paramsd phase")?;
    compare_lane(lane, input, phase, report)
}

/// Validate complete native phase evidence before the controller writes a
/// successful top-level receipt. The existing fresh-Python comparison remains
/// a separate gate while the historical source is present.
pub fn compare_before_receipt(
    native: &Path,
    daemon: &Path,
    hook: &Path,
    continuous_input: &Path,
    restart_input: &Path,
) -> Result<()> {
    let pins = read(&native.join("source-identities.json"))?;
    require_equal(
        pins["unchanged_after_run"].clone(),
        json!(true),
        "source identity stability",
    )?;
    for (key, path) in [
        ("daemon", daemon),
        ("phase_hook", hook),
        ("continuous_input", continuous_input),
        ("restart_input", restart_input),
    ] {
        require_equal(pins[key].clone(), json!(sha(&fs::read(path)?)), key)?;
    }
    for lane in &LANES {
        let phase = native.join(lane.name);
        let native_result = read(&phase.join("result.json"))?;
        compare_lane(
            lane,
            if lane.name == "continuous" {
                continuous_input
            } else {
                restart_input
            },
            &phase,
            &native_result,
        )?;
    }
    require_equal(
        fs::read(native.join("continuous/saved.bin"))?,
        fs::read(native.join("restart/saved.bin"))?,
        "native restart saved packet",
    )?;
    require_equal(
        LANES[0].saved,
        LANES[1].saved,
        "original restart saved packet",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independently_sealed_trace_and_corruption_rejection() {
        for lane in &LANES {
            assert_eq!(
                source(lane).unwrap()["rows"].as_array().unwrap().len(),
                lane.count + 1
            );
        }
        assert!(event_dict(&[0]).is_err());
        assert!(compare(
            &json!({"event":{"x":1.0}}),
            &json!({"event":{"x":1.01}}),
            "case"
        )
        .is_err());
        assert!(compare(&json!([1, 2]), &json!([1]), "case").is_err());
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("input.json");
        let original = original_input(&LANES[0]).unwrap();
        fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
        compare_input(&LANES[0], &path).unwrap();
        let mut content = original.clone();
        content["frames"][1]["messages"][0][0] = json!(1);
        fs::write(&path, serde_json::to_vec(&content).unwrap()).unwrap();
        assert!(
            compare_input(&LANES[0], &path).is_err(),
            "changed packet accepted"
        );
        let mut order = original;
        order["frames"][1]["messages"]
            .as_array_mut()
            .unwrap()
            .swap(0, 1);
        fs::write(&path, serde_json::to_vec(&order).unwrap()).unwrap();
        assert!(
            compare_input(&LANES[0], &path).is_err(),
            "changed service order accepted"
        );
        let restart = original_input(&LANES[1]).unwrap();
        fs::write(&path, serde_json::to_vec(&restart).unwrap()).unwrap();
        compare_input(&LANES[1], &path).unwrap();
        let mut timestamp_only = restart.clone();
        let timestamp = timestamp_only["seed"]["LiveParametersV2"][16]
            .as_u64()
            .unwrap();
        timestamp_only["seed"]["LiveParametersV2"][16] = json!(timestamp ^ 1);
        fs::write(&path, serde_json::to_vec(&timestamp_only).unwrap()).unwrap();
        compare_input(&LANES[1], &path).unwrap();
        let mut near_float = restart.clone();
        let low_bit = near_float["seed"]["LiveParametersV2"][48].as_u64().unwrap();
        near_float["seed"]["LiveParametersV2"][48] = json!(low_bit ^ 1);
        let original_bytes: Vec<u8> =
            serde_json::from_value(restart["seed"]["LiveParametersV2"].clone()).unwrap();
        let changed_bytes: Vec<u8> =
            serde_json::from_value(near_float["seed"]["LiveParametersV2"].clone()).unwrap();
        let before = f64::from(f32::from_le_bytes(
            original_bytes[48..52].try_into().unwrap(),
        ));
        let after = f64::from(f32::from_le_bytes(
            changed_bytes[48..52].try_into().unwrap(),
        ));
        assert!(
            (before - after).abs() < 2e-6 + 2e-6 * after.abs(),
            "negative control must remain below the former Event tolerance"
        );
        fs::write(&path, serde_json::to_vec(&near_float).unwrap()).unwrap();
        assert!(
            compare_input(&LANES[1], &path).is_err(),
            "non-timestamp f32 one-bit drift accepted"
        );
        let mut root_padding = restart;
        let zero = root_padding["seed"]["LiveParametersV2"][31]
            .as_u64()
            .unwrap();
        root_padding["seed"]["LiveParametersV2"][31] = json!(zero ^ 1);
        fs::write(&path, serde_json::to_vec(&root_padding).unwrap()).unwrap();
        assert!(
            compare_input(&LANES[1], &path).is_err(),
            "non-timestamp root-data bit accepted"
        );
        let mut bad_frame = original_bytes;
        bad_frame[0] = 1;
        assert!(normalized_restart_seed(&bad_frame).is_err());
        assert!(
            compare_phase_before_receipt("unknown", &path, temporary.path(), &json!({})).is_err()
        );
        assert!(compare_gps(&LANES[0], b"changed GPS").is_err());
        let mut saved = LANES[0].saved.to_vec();
        saved.truncate(1);
        assert!(compare_saved(&LANES[0], &saved).is_err());
        let original_result: Value = serde_json::from_slice(LANES[0].result).unwrap();
        let mut native = original_result.clone();
        native["no_python_runtime"] = json!(true);
        compare_phase_result(&LANES[0], &original_result, &native).unwrap();
        for (key, bad) in [
            ("publications", json!(1201)),
            ("signal", json!(15)),
            ("startup_poll", json!({})),
            ("pass", json!(false)),
            ("exit", json!(1)),
        ] {
            let mut changed = native.clone();
            changed[key] = bad;
            assert!(
                compare_phase_result(&LANES[0], &original_result, &changed).is_err(),
                "{key} corruption accepted"
            );
        }
        let original_restart: Value = serde_json::from_slice(LANES[1].result).unwrap();
        let mut restart_report = original_restart.clone();
        restart_report["no_python_runtime"] = json!(true);
        restart_report["persisted_packet_index"] = json!(null);
        compare_phase_result(&LANES[1], &original_restart, &restart_report).unwrap();
        restart_report["persisted_packet_index"] = json!(1199);
        assert!(compare_phase_result(&LANES[1], &original_restart, &restart_report).is_err());
    }
}
