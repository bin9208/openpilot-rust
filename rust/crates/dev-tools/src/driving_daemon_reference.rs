//! Independent original-only driving daemon observation and Cereal comparator.
//! The capture is an external Actions artifact, never generated from native output.
use crate::seal::Result;
use capnp::{dynamic_struct, dynamic_value::Reader as Dynamic};
use openpilot_cereal::log_capnp::event;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{Cursor, Read},
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
};

const MAX_MANIFEST: u64 = 1_000_000;
const MAX_FILE: u64 = 16_000_000;
const MAX_TOTAL: u64 = 512_000_000;
const OUTPUTS: [&str; 3] = ["modelV2", "drivingModelData", "cameraOdometry"];

pub struct Original {
    root: PathBuf,
    manifest: Value,
    manifest_hash: String,
    root_identity: (u64, u64),
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn relative(path: &str) -> Result<&Path> {
    let path = Path::new(path);
    if path
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
    {
        Ok(path)
    } else {
        Err(format!("invalid original capture member: {}", path.display()).into())
    }
}

fn read_regular(path: &Path, max: u64) -> Result<Vec<u8>> {
    let file = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?;
    let file = File::from(file);
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > max {
        return Err(format!("invalid original capture file: {}", path.display()).into());
    }
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 != metadata.len() || bytes.len() as u64 > max {
        return Err(format!("original capture file changed: {}", path.display()).into());
    }
    Ok(bytes)
}

fn walk(root: &Path, directory: &Path, names: &mut BTreeSet<String>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(format!("original capture contains symlink: {}", path.display()).into());
        }
        if kind.is_dir() {
            walk(root, &path, names)?;
        } else if kind.is_file() {
            names.insert(
                path.strip_prefix(root)?
                    .to_str()
                    .ok_or("non-UTF8 original member")?
                    .into(),
            );
        } else {
            return Err(
                format!("original capture contains special file: {}", path.display()).into(),
            );
        }
    }
    Ok(())
}

impl Original {
    /// The manifest digest is fixed by the caller from the accepted cloud artifact.
    pub fn open(root: &Path, expected_manifest_sha256: &str) -> Result<Self> {
        let metadata = fs::symlink_metadata(root)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("original capture root must be a real directory".into());
        }
        let root = root.to_owned();
        let raw = read_regular(&root.join("manifest.json"), MAX_MANIFEST)?;
        if sha(&raw) != expected_manifest_sha256 {
            return Err("original capture manifest digest changed".into());
        }
        let manifest: Value = serde_json::from_slice(&raw)?;
        let files = manifest["files"]
            .as_object()
            .ok_or("missing capture file inventory")?;
        if files.len() < 1_000 || files.len() > 2_000 {
            return Err("unexpected driving capture file count".into());
        }
        let mut names = BTreeSet::new();
        walk(&root, &root, &mut names)?;
        names.remove("manifest.json");
        if names != files.keys().cloned().collect() {
            return Err("original capture member roster changed".into());
        }
        let mut total = 0_u64;
        for (name, entry) in files {
            relative(name)?;
            let bytes = read_regular(&root.join(name), MAX_FILE)?;
            let length = entry["bytes"]
                .as_u64()
                .ok_or("invalid capture byte count")?;
            let digest = entry["sha256"].as_str().ok_or("invalid capture SHA")?;
            if bytes.len() as u64 != length || sha(&bytes) != digest {
                return Err(format!("original capture member changed: {name}").into());
            }
            total = total.checked_add(length).ok_or("capture length overflow")?;
        }
        if total > MAX_TOTAL {
            return Err("original capture exceeds bounded size".into());
        }
        let scenarios = manifest["scenarios"]
            .as_array()
            .ok_or("missing scenarios")?;
        let expected = [
            (1344, "dual", 12),
            (1344, "road", 4),
            (1344, "wide", 4),
            (1928, "dual", 12),
            (1928, "road", 4),
            (1928, "wide", 4),
        ];
        if scenarios.len() != expected.len() {
            return Err("original capture lacks six scenarios".into());
        }
        let mut publications = 0;
        for (case, (width, mode, count)) in scenarios.iter().zip(expected) {
            if case["resolution"][0] != width
                || case["mode"] != mode
                || case["frame_count"] != count
            {
                return Err("original capture case order changed".into());
            }
            let frames = case["frames"].as_array().ok_or("missing frame rows")?;
            if frames.len() != count {
                return Err("original capture frame count changed".into());
            }
            let warmup = &case["warmup"];
            if warmup["frame_id"] != 2 || warmup["prepare_only"] != true {
                return Err("original warmup schedule changed".into());
            }
            let warmup_packed = warmup["packed"]
                .as_str()
                .ok_or("missing warmup packed input")?;
            if !files.contains_key(warmup_packed) {
                return Err("warmup packed input not inventoried".into());
            }
            for (index, frame) in frames.iter().enumerate() {
                if frame["index"] != index
                    || frame["frame_id"] != index + 3 + usize::from(index >= 9)
                    || frame["topics"].as_object().is_none_or(|topics| {
                        topics.len() != 8 + usize::from([1, 4, 8].contains(&index))
                            || [
                                "deviceState",
                                "carState",
                                "roadCameraState",
                                "driverMonitoringState",
                                "carControl",
                                "liveDelay",
                                "carrotMan",
                                "radarState",
                            ]
                            .iter()
                            .any(|name| !topics.contains_key(*name))
                            || topics.contains_key("liveCalibration") != [1, 4, 8].contains(&index)
                    })
                {
                    return Err("original frame schedule changed".into());
                }
                let outputs = frame["publications"]
                    .as_object()
                    .ok_or("invalid publication row")?;
                let has_output = !outputs.is_empty();
                if has_output != (mode != "dual" || index != 9) {
                    return Err("original prepare-only schedule changed".into());
                }
                if has_output {
                    if outputs.len() != OUTPUTS.len() {
                        return Err("extra or missing original output topic".into());
                    }
                    for topic in OUTPUTS {
                        if frame["publications"][topic]["packet"].as_str().is_none()
                            || frame["publications"][topic]["decoded"].as_str().is_none()
                        {
                            return Err("missing original publication".into());
                        }
                    }
                    publications += OUTPUTS.len();
                }
                let packed = frame["packed"].as_str().ok_or("missing packed input")?;
                let packed_hash = frame["packed_sha256"]
                    .as_str()
                    .ok_or("missing packed SHA")?;
                if files.get(packed).and_then(|v| v["sha256"].as_str()) != Some(packed_hash) {
                    return Err(format!(
                        "original packed input was copied after mutation: {packed}"
                    )
                    .into());
                }
            }
        }
        if publications != 114
            || manifest["comparison_policy"]["captured_model_execution_time_sentinel"] != 0.5
            || manifest["comparison_policy"]["sentinel_not_independent_timing_answer"].as_bool()
                != Some(true)
        {
            return Err("original publication roster or timing policy changed".into());
        }
        Ok(Self {
            root,
            manifest,
            manifest_hash: expected_manifest_sha256.into(),
            root_identity: (metadata.dev(), metadata.ino()),
        })
    }

    pub fn scenarios(&self) -> &[Value] {
        self.manifest["scenarios"]
            .as_array()
            .expect("validated scenarios")
    }

    pub fn manifest(&self) -> &Value {
        &self.manifest
    }

    pub fn read(&self, name: &str) -> Result<Vec<u8>> {
        relative(name)?;
        let entry = self.manifest["files"]
            .as_object()
            .and_then(|files| files.get(name))
            .ok_or("unlisted original capture member")?;
        let bytes = read_regular(&self.root.join(name), MAX_FILE)?;
        let actual_sha = sha(&bytes);
        if entry["bytes"].as_u64() != Some(bytes.len() as u64)
            || entry["sha256"].as_str() != Some(actual_sha.as_str())
        {
            return Err(format!("original capture member changed: {name}").into());
        }
        Ok(bytes)
    }

    pub fn verify(&self) -> Result<()> {
        let metadata = fs::symlink_metadata(&self.root)?;
        if !metadata.is_dir() || (metadata.dev(), metadata.ino()) != self.root_identity {
            return Err("original capture root replaced".into());
        }
        if sha(&read_regular(
            &self.root.join("manifest.json"),
            MAX_MANIFEST,
        )?) != self.manifest_hash
        {
            return Err("original capture manifest replaced".into());
        }
        let mut names = BTreeSet::new();
        walk(&self.root, &self.root, &mut names)?;
        names.remove("manifest.json");
        let files = self.manifest["files"]
            .as_object()
            .ok_or("missing inventory")?;
        if names != files.keys().cloned().collect() {
            return Err("original capture member roster changed".into());
        }
        for name in files.keys() {
            self.read(name)?;
        }
        Ok(())
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        write!(&mut out, "{byte:02x}").expect("writing to String is infallible");
    }
    out
}

fn number(value: f64) -> Value {
    if value.is_finite() {
        json!(value)
    } else {
        json!({"float64_bits":hex(&value.to_be_bytes())})
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
        Dynamic::Data(v) => json!({"bytes_hex":hex(v)}),
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

pub fn event_dict(bytes: &[u8]) -> Result<Value> {
    let message = capnp::serialize::read_message(
        &mut Cursor::new(bytes),
        capnp::message::ReaderOptions::new(),
    )?;
    project(message.get_root::<event::Reader<'_>>()?.into())
}

fn compare(expected: &Value, actual: &Value, path: &str) -> Result<usize> {
    match (expected, actual) {
        (Value::Object(left), Value::Object(right))
            if left.len() == 1
                && right.len() == 1
                && left.contains_key("float64_bits")
                && right.contains_key("float64_bits") =>
        {
            let bits = |value: &Value| -> Result<f64> {
                let text = value
                    .as_str()
                    .ok_or("invalid original nonfinite float bits")?;
                let raw = u64::from_str_radix(text, 16)?;
                Ok(f64::from_bits(raw))
            };
            let a = bits(&left["float64_bits"])?;
            let b = bits(&right["float64_bits"])?;
            if a == b || (a.is_nan() && b.is_nan()) {
                Ok(1)
            } else {
                Err(format!("{path}: nonfinite values differ").into())
            }
        }
        (Value::Object(left), Value::Object(right)) => {
            if left.keys().collect::<Vec<_>>() != right.keys().collect::<Vec<_>>() {
                return Err(format!("{path}: publication keys changed").into());
            }
            left.iter().try_fold(0_usize, |count, (name, value)| {
                Ok(count + compare(value, &right[name], &format!("{path}.{name}"))?)
            })
        }
        (Value::Array(left), Value::Array(right)) => {
            if left.len() != right.len() {
                return Err(format!("{path}: array length changed").into());
            }
            left.iter()
                .zip(right)
                .enumerate()
                .try_fold(0_usize, |count, (index, (a, b))| {
                    Ok(count + compare(a, b, &format!("{path}[{index}]"))?)
                })
        }
        (Value::Number(left), Value::Number(right)) if left.is_f64() => {
            let a = left.as_f64().ok_or("reference float")?;
            let b = right.as_f64().ok_or("candidate float")?;
            let tolerance: f64 = if path.contains("Coefficients") {
                2e-5
            } else {
                1e-6
            };
            if !(a == b
                || (a.is_finite()
                    && b.is_finite()
                    && (a - b).abs() <= tolerance.max(tolerance * a.abs().max(b.abs()))))
            {
                return Err(format!("{path}: {a} != {b}").into());
            }
            Ok(1)
        }
        _ if expected == actual => Ok(1),
        _ => Err(format!("{path}: original and native fields differ").into()),
    }
}

pub struct Comparison {
    pub fields: usize,
    pub model_execution_time: f64,
    pub log_mono_time: u64,
}

/// Call on each real IPC publication. Only the two declared timing values and
/// top-level logMonoTime are dynamic; all other fields use the original policy.
pub fn compare_packet(
    expected: &Value,
    packet: &[u8],
    topic: &str,
    send_start_ns: u64,
    received_ns: u64,
    raw_prediction: Option<&[u8]>,
) -> Result<Comparison> {
    if !OUTPUTS.contains(&topic) {
        return Err("unexpected driving publication topic".into());
    }
    let mut reference = expected.clone();
    let mut observed = event_dict(packet)?;
    let expected_root = reference
        .as_object_mut()
        .ok_or("reference Event not an object")?;
    let actual_root = observed
        .as_object_mut()
        .ok_or("candidate Event not an object")?;
    let reference_topic = expected_root.get(topic).ok_or("reference topic missing")?;
    let actual_topic = actual_root.get(topic).ok_or("candidate topic missing")?;
    if reference_topic.is_null() || actual_topic.is_null() {
        return Err("publication topic is null".into());
    }
    let stamp = actual_root
        .remove("logMonoTime")
        .and_then(|v| v.as_u64())
        .ok_or("missing publication clock")?;
    if !(send_start_ns..=received_ns).contains(&stamp) {
        return Err("publication clock outside send/receive window".into());
    }
    expected_root.remove("logMonoTime");
    let execution = if topic == "cameraOdometry" {
        0.0
    } else {
        let value = actual_root
            .get_mut(topic)
            .and_then(Value::as_object_mut)
            .and_then(|map| map.remove("modelExecutionTime"))
            .and_then(|v| v.as_f64())
            .ok_or("missing model execution time")?;
        reference
            .get_mut(topic)
            .and_then(Value::as_object_mut)
            .and_then(|map| map.remove("modelExecutionTime"))
            .ok_or("reference execution sentinel missing")?;
        if !(0.0 < value && value < 30.0) {
            return Err("model execution time outside original bound".into());
        }
        value
    };
    if let Some(raw) = raw_prediction {
        if topic != "modelV2" {
            return Err("raw prediction belongs only to modelV2".into());
        }
        let found = actual_root[topic]["rawPredictions"]["bytes_hex"]
            .as_str()
            .ok_or("missing raw predictions")?;
        if found != hex(raw) {
            return Err("raw predictions differ from original model bytes".into());
        }
    }
    // The original recursive comparator also counts the independently checked
    // clock and (for the two model rows) execution-time leaves.
    let fields =
        compare(&reference, &observed, topic)? + 1 + usize::from(topic != "cameraOdometry");
    Ok(Comparison {
        fields,
        model_execution_time: execution,
        log_mono_time: stamp,
    })
}
