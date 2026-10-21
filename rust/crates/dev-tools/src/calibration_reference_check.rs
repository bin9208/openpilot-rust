//! Compare the real calibration_trace stream with an independent, original-only
//! calibrationd capture. The fixture is source evidence, never candidate output.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    qa_process::{CaptureError, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use capnp::{dynamic_struct, dynamic_value::Reader as Datum};
use flate2::read::GzDecoder;
use openpilot_cereal::log_capnp::event;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env,
    ffi::OsString,
    fs,
    io::{BufWriter, Cursor, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

const ORIGINAL: &[u8] =
    include_bytes!("../tests/fixtures/calibration-estimator-original-04ee2c/original.jsonl.gz");
const MANIFEST: &[u8] =
    include_bytes!("../tests/fixtures/calibration-estimator-original-04ee2c/manifest.json");
const ORIGINAL_SHA: &str = "9ea4da7d3216e19a2fcaf4043c928d59f7b4014e2aa7f6ee2b4c401b3f045de8";
const MANIFEST_SHA: &str = "a8144a49c71ffae8c46b595d0fde684b71784f2574149bb81fb65c62c0dde663";
const SOURCE_COMMIT: &str = "5bcfae4482d9c4dcb1da46e1086bea609e0c9688";
const STEPS: usize = 15_128;
const MAX_TRACE: u64 = 32 * 1024 * 1024;
const RETIRED_SOURCES: [&str; 2] = [
    "rust/tools/check_calibration_reference.py",
    "rust/tools/calibration_cases.py",
];

struct SourceSeal {
    path: PathBuf,
    name: String,
    hash: String,
    absent: bool,
}

fn source_seals(root: &Path, sources: &Map<String, Value>) -> Result<Vec<SourceSeal>> {
    let mut seals = Vec::with_capacity(sources.len());
    for (name, hash) in sources {
        let path = root.join(name);
        let hash = hash.as_str().ok_or("source hash not text")?.to_owned();
        let absent = match fs::symlink_metadata(&path) {
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && RETIRED_SOURCES.contains(&name.as_str()) =>
            {
                true
            }
            Ok(metadata) => {
                require(
                    metadata.file_type().is_file(),
                    format!("source not regular: {name}"),
                )?;
                require(
                    sha(&fs::read(&path)?) == hash,
                    format!("original source changed: {name}"),
                )?;
                false
            }
            Err(error) => return Err(error.into()),
        };
        seals.push(SourceSeal {
            path,
            name: name.clone(),
            hash,
            absent,
        });
    }
    Ok(seals)
}

fn verify_source_seals(seals: &[SourceSeal]) -> Result<()> {
    for seal in seals {
        match fs::symlink_metadata(&seal.path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && seal.absent => {}
            Ok(metadata) if !seal.absent && metadata.file_type().is_file() => {
                require(
                    sha(&fs::read(&seal.path)?) == seal.hash,
                    format!("source changed during run: {}", seal.name),
                )?;
            }
            _ => return Err(format!("source presence changed during run: {}", seal.name).into()),
        }
    }
    Ok(())
}

pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub output: PathBuf,
}

pub fn parse(args: &[OsString]) -> Result<Option<Options>> {
    let parsed = scan_os(
        args,
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if parsed.help {
        return Ok(None);
    }
    Ok(Some(Options {
        root: env::var_os("OPENPILOT_RUST_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")),
        binary: PathBuf::from(&parsed.values["--binary"]),
        output: PathBuf::from(&parsed.values["--output"]),
    }))
}

pub fn cli() -> Result<()> {
    let Some(options) = parse(&env::args_os().skip(1).collect::<Vec<_>>())? else {
        println!("usage: check-calibration-reference --binary BINARY --output DIRECTORY");
        return Ok(());
    };
    let report = check(&options, &mut NativeRunner)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn require(condition: bool, message: impl Into<String>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into().into())
    }
}

fn obj<'a>(value: &'a Value, context: &str) -> Result<&'a Map<String, Value>> {
    value
        .as_object()
        .ok_or_else(|| format!("{context}: expected object").into())
}

fn field<'a>(value: &'a Value, name: &str) -> Result<&'a Value> {
    obj(value, name)?
        .get(name)
        .ok_or_else(|| format!("missing {name}").into())
}

fn number(value: &Value) -> Result<f64> {
    let text = value.as_str().ok_or("expected original decimal string")?;
    Ok(match text {
        "nan" | "NaN" => f64::NAN,
        "inf" | "Infinity" => f64::INFINITY,
        "-inf" | "-Infinity" => f64::NEG_INFINITY,
        _ => text.parse()?,
    })
}

fn close(actual: f64, expected: f64, wire: bool) -> bool {
    if expected.is_nan() {
        return actual.is_nan();
    }
    if expected.is_infinite() {
        return actual == expected;
    }
    if !actual.is_finite() {
        return false;
    }
    let tolerance = if wire {
        let rounded = expected as f32;
        let adjacent = |up: bool| -> f32 {
            if rounded.is_nan() {
                return rounded;
            }
            if rounded == f32::INFINITY && up || rounded == f32::NEG_INFINITY && !up {
                return rounded;
            }
            if rounded == 0.0 {
                return f32::from_bits(if up { 1 } else { 0x8000_0001 });
            }
            let bits = rounded.to_bits();
            f32::from_bits(if up == rounded.is_sign_positive() {
                bits + 1
            } else {
                bits - 1
            })
        };
        [adjacent(false), adjacent(true)]
            .into_iter()
            .filter(|v| v.is_finite())
            .fold(2e-12_f64, |tol, v| tol.max((f64::from(v) - expected).abs()))
    } else {
        2e-12 + 2e-12 * expected.abs()
    };
    (actual - expected).abs() <= tolerance
}

fn compare_float(expected: &Value, actual: &Value, wire: bool, path: &str) -> Result<()> {
    let left = number(expected)?;
    let right = number(actual)?;
    require(
        close(right, left, wire),
        format!("{path}: numerical difference: {left:?} != {right:?}"),
    )
}

fn compare_floats(expected: &Value, actual: &Value, wire: bool, path: &str) -> Result<()> {
    let left = expected.as_array().ok_or("expected original float array")?;
    let right = actual.as_array().ok_or("expected candidate float array")?;
    require(
        left.len() == right.len(),
        format!("{path}: array length differs"),
    )?;
    for (index, (a, b)) in left.iter().zip(right).enumerate() {
        compare_float(a, b, wire, &format!("{path}[{index}]"))?;
    }
    Ok(())
}

fn compare_state(expected: &Value, actual: &Value) -> Result<()> {
    let left = obj(expected, "original state")?;
    let right = obj(actual, "candidate state")?;
    const FIELDS: [&str; 11] = [
        "rpy",
        "wide",
        "height",
        "spread",
        "old_rpy",
        "old_weight",
        "valid_indices",
        "idx",
        "block_idx",
        "valid_blocks",
        "status",
    ];
    // The response also carries packet and action-specific fields. Its full
    // top-level key set is checked by compare_row before this state comparison.
    require(
        FIELDS.iter().all(|name| right.contains_key(*name)),
        "candidate state field set changed",
    )?;
    require(
        left.len() == FIELDS.len() && FIELDS.iter().all(|name| left.contains_key(*name)),
        "original state field set changed",
    )?;
    for name in ["rpy", "wide", "spread", "old_rpy"] {
        compare_floats(&left[name], &right[name], false, name)?;
    }
    compare_float(&left["height"], &right["height"], false, "height")?;
    require(
        number(&left["old_weight"])? == number(&right["old_weight"])?,
        "old_weight differs",
    )?;
    for name in [
        "valid_indices",
        "idx",
        "block_idx",
        "valid_blocks",
        "status",
    ] {
        require(left[name] == right[name], format!("{name} differs"))?;
    }
    Ok(())
}

// pycapnp Event.to_dict() retains active union, all scalar fields, and present
// pointer fields. This independent projection follows that exact structural rule.
fn project(value: Datum<'_>) -> Result<Value> {
    Ok(match value {
        Datum::Void => Value::Null,
        Datum::Bool(v) => json!(v),
        Datum::Int8(v) => json!(v),
        Datum::Int16(v) => json!(v),
        Datum::Int32(v) => json!(v),
        Datum::Int64(v) => json!(v),
        Datum::UInt8(v) => json!(v),
        Datum::UInt16(v) => json!(v),
        Datum::UInt32(v) => json!(v),
        Datum::UInt64(v) => json!(v),
        Datum::Float32(v) => json!(format!("{:.17}", f64::from(v))),
        Datum::Float64(v) => json!(format!("{v:.17}")),
        Datum::Text(v) => json!(v.to_str()?),
        Datum::Enum(v) => json!(v
            .get_enumerant()?
            .ok_or("unknown cereal enum")?
            .get_proto()
            .get_name()?
            .to_str()?),
        Datum::List(v) => Value::Array(v.iter().map(|item| project(item?)).collect::<Result<_>>()?),
        Datum::Struct(v) => Value::Object(structure(v)?),
        Datum::Data(_) | Datum::AnyPointer(_) | Datum::Capability(_) => {
            return Err("unexpected cereal pointer".into())
        }
    })
}

fn structure(value: dynamic_struct::Reader<'_>) -> Result<Map<String, Value>> {
    let mut result = Map::new();
    if let Some(active) = value.which()? {
        result.insert(
            active.get_proto().get_name()?.to_str()?.into(),
            project(value.get(active)?)?,
        );
    } else if !value.get_schema().get_union_fields()?.is_empty() {
        return Err("unknown cereal union discriminant".into());
    }
    for member in value.get_schema().get_non_union_fields()? {
        if value.has(member)? {
            result.insert(
                member.get_proto().get_name()?.to_str()?.into(),
                project(value.get(member)?)?,
            );
        }
    }
    Ok(result)
}

fn decode_packet(bytes: &[u8]) -> Result<Value> {
    let mut cursor = Cursor::new(bytes);
    let message =
        capnp::serialize::read_message(&mut cursor, capnp::message::ReaderOptions::new())?;
    require(
        cursor.position() as usize == bytes.len(),
        "trailing packet bytes",
    )?;
    let root = message.get_root::<event::Reader<'_>>()?;
    let event::LiveCalibration(inner) = root.which()? else {
        return Err("packet service is not liveCalibration".into());
    };
    inner?;
    project(root.into())
}

fn unhex(hex: &str) -> Result<Vec<u8>> {
    require(
        hex.len().is_multiple_of(2) && hex.len() <= 64 * 1024,
        "invalid packet hex length",
    )?;
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |b: u8| -> Result<u8> {
                match b {
                    b'0'..=b'9' => Ok(b - b'0'),
                    b'a'..=b'f' => Ok(b - b'a' + 10),
                    _ => Err("invalid lowercase packet hex".into()),
                }
            };
            Ok((digit(pair[0])? << 4) | digit(pair[1])?)
        })
        .collect()
}

fn compare_packet(expected: &Value, actual: &Value, path: &str) -> Result<usize> {
    match expected {
        Value::Object(left) => {
            let right = obj(actual, path)?;
            require(
                left.len() == right.len() && left.keys().all(|name| right.contains_key(name)),
                format!("{path}: packet fields differ"),
            )?;
            let mut count = 0;
            for (name, value) in left {
                count += compare_packet(value, &right[name], &format!("{path}.{name}"))?;
            }
            Ok(count)
        }
        Value::Array(left) => {
            let right = actual.as_array().ok_or("packet array missing")?;
            require(
                left.len() == right.len(),
                format!("{path}: packet array length differs"),
            )?;
            for (index, (a, b)) in left.iter().zip(right).enumerate() {
                compare_float(a, b, true, &format!("{path}[{index}]"))?;
            }
            Ok(left.len())
        }
        _ => {
            require(expected == actual, format!("{path}: packet scalar differs"))?;
            Ok(1)
        }
    }
}

fn capture() -> Result<(Value, Vec<Value>)> {
    require(
        sha(ORIGINAL) == ORIGINAL_SHA && sha(MANIFEST) == MANIFEST_SHA,
        "calibration original-only fixture changed",
    )?;
    let manifest: Value = serde_json::from_slice(MANIFEST)?;
    require(
        manifest["source_commit"] == SOURCE_COMMIT
            && manifest["trace_sha256"] == ORIGINAL_SHA
            && manifest["steps"].as_u64() == Some(STEPS as u64),
        "calibration original manifest changed",
    )?;
    let mut decoded = Vec::new();
    GzDecoder::new(ORIGINAL)
        .take(MAX_TRACE + 1)
        .read_to_end(&mut decoded)?;
    require(
        (decoded.len() as u64) <= MAX_TRACE,
        "calibration original trace exceeds bound",
    )?;
    let rows = decoded
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(serde_json::from_slice)
        .collect::<std::result::Result<Vec<Value>, _>>()?;
    require(
        rows.len() == STEPS,
        "calibration original trace step count changed",
    )?;
    let mut scenarios = BTreeMap::<String, usize>::new();
    let mut errors = 0;
    let mut packets = 0;
    let mut writes = 0;
    for (index, row) in rows.iter().enumerate() {
        require(
            row["step"].as_u64() == Some((index + 1) as u64),
            format!("calibration original step {} changed", index + 1),
        )?;
        let scenario = row["scenario"]
            .as_str()
            .ok_or("original scenario missing")?;
        *scenarios.entry(scenario.into()).or_default() += 1;
        let original = field(row, "original")?;
        let command = field(row, "command")?;
        require(
            matches!(command["action"].as_str(), Some("reset" | "update")),
            "original action changed",
        )?;
        let saved = original["writes_hex"]
            .as_array()
            .ok_or("original writes missing")?;
        writes += saved.len();
        if original.get("error_class").is_some() {
            require(
                original["error_message"].as_str().is_some(),
                "original error message missing",
            )?;
            errors += 1;
        } else {
            require(
                original["state"].is_object()
                    && original["packet_fields"].is_object()
                    && original["packet_hex"].as_str().is_some()
                    && original["persist"].is_boolean(),
                "original successful observation incomplete",
            )?;
            packets += 1;
        }
    }
    require(
        manifest["source_errors"].as_u64() == Some(errors as u64)
            && manifest["packets"].as_u64() == Some(packets as u64)
            && manifest["persistence_packets"].as_u64() == Some(writes as u64),
        "calibration original counts changed",
    )?;
    let manifest_scenarios = obj(&manifest["scenarios"], "manifest scenarios")?;
    require(
        scenarios.len() == manifest_scenarios.len()
            && scenarios
                .iter()
                .all(|(name, count)| manifest_scenarios[name].as_u64() == Some(*count as u64)),
        "calibration scenario counts changed",
    )?;
    Ok((manifest, rows))
}

fn compare_row(row: &Value, actual: &Value) -> Result<usize> {
    let original = field(row, "original")?;
    let actual_fields = obj(actual, "native observation")?;
    if original.get("error_class").is_some() {
        require(
            actual_fields.len() == 1 && actual["error"].as_str().is_some_and(|s| !s.is_empty()),
            "native error position or form differs",
        )?;
        return Ok(0);
    }
    require(
        !actual_fields.contains_key("error"),
        "unexpected native error",
    )?;
    let action = row["command"]["action"]
        .as_str()
        .ok_or("original action missing")?;
    let allowed: &[&str] = if action == "reset" {
        &[
            "block_idx",
            "height",
            "idx",
            "old_rpy",
            "old_weight",
            "packet",
            "rpy",
            "saved_error",
            "spread",
            "status",
            "valid_blocks",
            "valid_indices",
            "wide",
        ]
    } else {
        &[
            "accepted",
            "block_idx",
            "height",
            "idx",
            "old_rpy",
            "old_weight",
            "packet",
            "persist",
            "rpy",
            "spread",
            "status",
            "valid_blocks",
            "valid_indices",
            "wide",
        ]
    };
    require(
        actual_fields.len() == allowed.len()
            && allowed.iter().all(|name| actual_fields.contains_key(*name)),
        "native output field set differs",
    )?;
    compare_state(&original["state"], actual)?;
    if action == "update" {
        require(
            original["persist"] == actual["persist"],
            "persistence step differs",
        )?;
        match (&original["accepted"], &actual["accepted"]) {
            (Value::Null, Value::Null) => {}
            (left, right) => compare_floats(left, right, false, "accepted")?,
        }
    } else {
        require(
            original["saved_error"].is_boolean()
                && original["saved_error"] == actual["saved_error"],
            "cached Params failure differs",
        )?;
    }
    let packet = actual["packet"]
        .as_array()
        .ok_or("native packet missing")?
        .iter()
        .map(|byte| -> Result<u8> {
            Ok(byte
                .as_u64()
                .and_then(|v| u8::try_from(v).ok())
                .ok_or("invalid native packet byte")?)
        })
        .collect::<Result<Vec<_>>>()?;
    let projected = decode_packet(&packet)?;
    let count = compare_packet(&original["packet_fields"], &projected, "packet")?;
    let writes = original["writes_hex"]
        .as_array()
        .ok_or("original writes missing")?;
    require(
        writes.len() <= 1,
        "more than one original Params write in one step",
    )?;
    require(
        writes.is_empty() || original["persist"].as_bool() == Some(true),
        "original write lacks persist flag",
    )?;
    if let Some(write) = writes.first() {
        let saved = decode_packet(&unhex(write.as_str().ok_or("original write hex missing")?)?)?;
        require(
            saved["logMonoTime"].as_u64() == Some(0) && saved["valid"].as_bool() == Some(true),
            "original saved Event header changed",
        )?;
        let mut candidate_saved = projected;
        candidate_saved["logMonoTime"] = json!(0);
        candidate_saved["valid"] = json!(true);
        compare_packet(&saved, &candidate_saved, "saved packet")?;
    }
    Ok(count)
}

pub fn check(options: &Options, runner: &mut impl Runner) -> Result<Value> {
    let (manifest, rows) = capture()?;
    let sources = obj(&manifest["source_sha256"], "original source hashes")?;
    let source_seals = source_seals(&options.root, sources)?;
    let binary = fs::canonicalize(&options.binary)?;
    require(
        fs::symlink_metadata(&binary)?.file_type().is_file(),
        "calibration candidate is not a regular ELF",
    )?;
    let bytes = fs::read(&binary)?;
    require(
        bytes.starts_with(b"\x7fELF\x02\x01"),
        "calibration candidate is not ELF64",
    )?;
    let binary_sha = sha(&bytes);
    let mut input = Vec::new();
    for row in &rows {
        serde_json::to_writer(&mut input, field(row, "command")?)?;
        input.push(b'\n');
    }
    require(
        input.len() < 16 * 1024 * 1024,
        "calibration request stream exceeds bound",
    )?;
    if let Some(parent) = options
        .output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&options.output)?;
    fs::write(options.output.join("input.jsonl"), &input)?;
    let mut spec = CommandSpec::new_os(
        vec![binary.as_os_str().to_owned()],
        Duration::from_secs(1_200),
    );
    spec.input = Some(input);
    spec.stdout_limit = 64 * 1024 * 1024;
    spec.stderr_limit = 4 * 1024 * 1024;
    let output = match runner.run(&spec) {
        Ok(output) => output,
        Err(error) => {
            if let Some(CaptureError::Timeout { stdout, stderr, .. }) =
                error.downcast_ref::<CaptureError>()
            {
                fs::write(options.output.join("native.stdout.partial"), stdout)?;
                fs::write(options.output.join("native.stderr.partial"), stderr)?;
            }
            return Err(error);
        }
    };
    fs::write(options.output.join("native.stdout"), &output.stdout)?;
    fs::write(options.output.join("native.stderr"), &output.stderr)?;
    require(
        output.code == 0,
        format!("calibration native exit {}", output.code),
    )?;
    require(
        output.stderr.is_empty(),
        "calibration native stderr is not empty",
    )?;
    let mut lines = output
        .stdout
        .split(|byte| *byte == b'\n')
        .collect::<Vec<_>>();
    if lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    require(
        lines.len() == STEPS && lines.iter().all(|line| !line.is_empty()),
        format!(
            "calibration native returned {} of {STEPS} rows",
            lines.len()
        ),
    )?;
    let mut trace = BufWriter::new(fs::File::create(options.output.join("trace.jsonl"))?);
    let mut packet_fields = 0;
    let mut failures = 0;
    let mut persistence_steps = 0;
    let mut scenarios = BTreeMap::<String, usize>::new();
    let mut statuses = BTreeMap::<String, usize>::new();
    for (index, (row, line)) in rows.iter().zip(lines).enumerate() {
        let actual: Value = serde_json::from_slice(line)?;
        packet_fields += compare_row(row, &actual)
            .map_err(|error| format!("calibration step {}: {error}", index + 1))?;
        let scenario = row["scenario"]
            .as_str()
            .ok_or("original scenario missing")?;
        *scenarios.entry(scenario.to_owned()).or_default() += 1;
        if actual.get("error").is_some() {
            failures += 1;
        } else {
            let status = actual["status"].as_str().ok_or("native status missing")?;
            *statuses.entry(status.to_owned()).or_default() += 1;
            if actual["persist"].as_bool() == Some(true) {
                persistence_steps += 1;
            }
        }
        serde_json::to_writer(
            &mut trace,
            &json!({"scenario": scenario, "step": index + 1, "state": actual}),
        )?;
        trace.write_all(b"\n")?;
    }
    trace.flush()?;
    require(
        failures
            == manifest["source_errors"]
                .as_u64()
                .ok_or("original error count missing")? as usize
            && persistence_steps
                == manifest["persistence_packets"]
                    .as_u64()
                    .ok_or("original write count missing")? as usize
            && serde_json::to_value(&scenarios)? == manifest["scenarios"]
            && serde_json::to_value(&statuses)? == manifest["statuses"],
        "calibration report counts differ from original capture",
    )?;
    require(
        sha(&fs::read(&binary)?) == binary_sha,
        "calibration binary changed during run",
    )?;
    verify_source_seals(&source_seals)?;
    let report = json!({"result":"pass","steps":STEPS,"packet_fields":packet_fields,
        "expected_source_errors":failures,"persistence_steps":persistence_steps,
        "scenarios":scenarios,"statuses":statuses,
        "state_tolerance":manifest["state_tolerance"],"wire_tolerance":manifest["wire_tolerance"]});
    fs::write(
        options.output.join("report.json"),
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    let summary = json!({"original_trace_sha256":ORIGINAL_SHA,"native_binary_sha256":binary_sha,
        "original_source_commit":SOURCE_COMMIT});
    fs::write(
        options.output.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    #[test]
    fn only_two_historical_sources_may_be_absent_and_reappearance_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let required = "rust/tools/calibration_reference.py";
        let retired = RETIRED_SOURCES[0];
        let required_path = dir.path().join(required);
        fs::create_dir_all(required_path.parent().unwrap()).unwrap();
        fs::write(&required_path, b"pinned reference").unwrap();
        let mut sources = Map::new();
        sources.insert(required.into(), json!(sha(b"pinned reference")));
        sources.insert(retired.into(), json!(sha(b"old checker")));
        let seals = source_seals(dir.path(), &sources).unwrap();
        assert!(verify_source_seals(&seals).is_ok());
        fs::write(dir.path().join(retired), b"old checker").unwrap();
        assert!(
            verify_source_seals(&seals).is_err(),
            "reappearance accepted"
        );
        fs::remove_file(dir.path().join(retired)).unwrap();
        fs::write(&required_path, b"changed reference").unwrap();
        assert!(
            source_seals(dir.path(), &sources).is_err(),
            "changed mandatory source accepted"
        );
        fs::remove_file(&required_path).unwrap();
        assert!(
            source_seals(dir.path(), &sources).is_err(),
            "missing mandatory source accepted"
        );
    }

    #[test]
    fn cli_requires_both_paths_and_preserves_raw_unix_bytes() {
        for args in [
            vec![],
            vec![OsString::from("--binary"), OsString::from("a")],
        ] {
            assert!(parse(&args).is_err());
        }
        assert!(parse(&[OsString::from("--help")]).unwrap().is_none());
        assert!(parse(&[
            OsString::from("--binary"),
            OsString::from("a"),
            OsString::from("--output"),
            OsString::from("b"),
            OsString::from("--unknown"),
        ])
        .is_err());
        let binary = OsString::from_vec(b"candidate-\xff".to_vec());
        let output = OsString::from_vec(b"evidence-\xfe".to_vec());
        let options = parse(&[
            OsString::from("--binary"),
            binary.clone(),
            OsString::from("--output"),
            output.clone(),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(options.binary.as_os_str().as_bytes(), binary.as_bytes());
        assert_eq!(options.output.as_os_str().as_bytes(), output.as_bytes());
    }

    #[test]
    fn non_elf_candidate_is_rejected_before_nested_output_creation_or_launch() {
        struct Never;
        impl Runner for Never {
            fn run(&mut self, _: &CommandSpec) -> Result<crate::qa_process::Captured> {
                panic!("non-ELF candidate was launched")
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("candidate");
        fs::write(&binary, b"not an ELF").unwrap();
        let output = dir.path().join("nested").join("evidence");
        let options = Options {
            root: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."),
            binary,
            output: output.clone(),
        };
        let error = check(&options, &mut Never).unwrap_err();
        assert!(error.to_string().contains("not ELF64"), "{error}");
        assert!(!output.exists());
        assert!(!output.parent().unwrap().exists());
    }

    // These are negative controls for the comparator. Only check() executes a
    // newly built native ELF; none of these synthetic rows proves parity.
    fn candidate(row: &Value) -> Value {
        let original = &row["original"];
        let state = original["state"].as_object().unwrap();
        let mut result = state.clone();
        result.insert(
            "packet".into(),
            json!(unhex(original["packet_hex"].as_str().unwrap()).unwrap()),
        );
        if row["command"]["action"] == "update" {
            result.insert("accepted".into(), original["accepted"].clone());
            result.insert("persist".into(), original["persist"].clone());
        } else {
            result.insert("saved_error".into(), json!(false));
        }
        Value::Object(result)
    }

    fn changed_header(packet: &Value, valid: Option<bool>, time: Option<u64>) -> Value {
        let raw: Vec<u8> = packet
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u8)
            .collect();
        let message = capnp::serialize::read_message(raw.as_slice(), Default::default()).unwrap();
        let mut rewritten = capnp::message::Builder::new_default();
        rewritten
            .set_root(message.get_root::<event::Reader<'_>>().unwrap())
            .unwrap();
        let mut root = rewritten.get_root::<event::Builder<'_>>().unwrap();
        if let Some(value) = valid {
            root.set_valid(value);
        }
        if let Some(value) = time {
            root.set_log_mono_time(value);
        }
        json!(capnp::serialize::write_message_to_words(&rewritten))
    }

    #[test]
    fn sealed_original_has_entire_ordered_contract() {
        let (_, rows) = capture().unwrap();
        assert_eq!(rows.len(), STEPS);
        assert_eq!(
            rows.iter()
                .filter(|row| row["original"].get("error_class").is_some())
                .count(),
            30
        );
        assert_eq!(
            rows.iter()
                .filter(|row| row["original"]["persist"].as_bool() == Some(true))
                .count(),
            14
        );
        assert_eq!(
            rows.iter()
                .filter(|row| row["command"]["action"] == "reset")
                .count(),
            163
        );
    }

    #[test]
    fn mutation_controls_reject_header_state_shape_error_position_and_saved_write() {
        let (_, rows) = capture().unwrap();
        let reset = rows
            .iter()
            .find(|row| row["original"]["packet_hex"].is_string())
            .unwrap();
        let mut actual = candidate(reset);
        assert!(compare_row(reset, &actual).is_ok());
        actual["saved_error"] = json!(!reset["original"]["saved_error"].as_bool().unwrap());
        assert!(
            compare_row(reset, &actual).is_err(),
            "wrong cached Params failure accepted"
        );
        actual = candidate(reset);
        actual["packet"] = changed_header(&actual["packet"], Some(false), None);
        assert!(
            compare_row(reset, &actual).is_err(),
            "validity drift accepted"
        );
        actual = candidate(reset);
        actual["packet"] = changed_header(&actual["packet"], None, Some(19));
        assert!(
            compare_row(reset, &actual).is_err(),
            "timestamp drift accepted"
        );
        actual = candidate(reset);
        actual.as_object_mut().unwrap().remove("spread");
        assert!(
            compare_row(reset, &actual).is_err(),
            "missing state accepted"
        );
        actual = candidate(reset);
        actual["rpy"].as_array_mut().unwrap().pop();
        assert!(
            compare_row(reset, &actual).is_err(),
            "short state array accepted"
        );
        let expected_packet = &reset["original"]["packet_fields"];
        let mut projected =
            decode_packet(&unhex(reset["original"]["packet_hex"].as_str().unwrap()).unwrap())
                .unwrap();
        projected.as_object_mut().unwrap().remove("valid");
        assert!(
            compare_packet(expected_packet, &projected, "packet").is_err(),
            "omitted packet field accepted"
        );
        let mut projected =
            decode_packet(&unhex(reset["original"]["packet_hex"].as_str().unwrap()).unwrap())
                .unwrap();
        projected["liveCalibration"]["rpyCalib"]
            .as_array_mut()
            .unwrap()
            .pop();
        assert!(
            compare_packet(expected_packet, &projected, "packet").is_err(),
            "short packet array accepted"
        );
        assert!(
            compare_row(reset, &json!({"error":"false error"})).is_err(),
            "early error accepted"
        );

        let cached_failure = rows
            .iter()
            .find(|row| row["original"]["saved_error"] == true)
            .unwrap();
        let mut cached_actual = candidate(cached_failure);
        assert!(
            compare_row(cached_failure, &cached_actual).is_err(),
            "missing cached Params failure accepted"
        );
        cached_actual["saved_error"] = json!(true);
        assert!(compare_row(cached_failure, &cached_actual).is_ok());

        let error = rows
            .iter()
            .find(|row| row["original"]["error_class"].is_string())
            .unwrap();
        assert!(compare_row(error, &json!({"error":"native validation error"})).is_ok());
        assert!(
            compare_row(error, &candidate(reset)).is_err(),
            "missing error accepted"
        );

        let write = rows
            .iter()
            .find(|row| {
                row["original"]["writes_hex"]
                    .as_array()
                    .is_some_and(|v| !v.is_empty())
            })
            .unwrap();
        let baseline = candidate(write);
        assert!(compare_row(write, &baseline).is_ok());
        let mut missing = baseline.clone();
        missing["persist"] = json!(false);
        assert!(
            compare_row(write, &missing).is_err(),
            "missing persistence accepted"
        );
        let mut foreign = write.clone();
        let hex = foreign["original"]["writes_hex"][0]
            .as_str()
            .unwrap()
            .to_owned();
        let mut bytes = unhex(&hex).unwrap();
        bytes[0] ^= 1;
        foreign["original"]["writes_hex"][0] = json!(bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>());
        assert!(
            compare_row(&foreign, &baseline).is_err(),
            "changed saved packet accepted"
        );
    }
}
