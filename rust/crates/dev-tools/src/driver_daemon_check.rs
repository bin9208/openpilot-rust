//! Admission for the unchanged Python driver's independent packet observations.
//!
//! The native controller consumes this capture only after the current-source
//! original checker has completed all four cases in Actions.
//! Captured candidate `message-*.bin` files are never reference packets.
use crate::{
    driving_daemon_reference::event_dict,
    driving_logging_check::Collector,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    process::{kill_process, kill_process_group, Pid, Signal},
    time::{clock_gettime, ClockId},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs::{self, File},
    io::{Read, Write},
    os::{
        fd::OwnedFd,
        unix::{
            ffi::OsStrExt,
            fs::{FileTypeExt, MetadataExt},
            process::CommandExt,
        },
    },
    path::{Component, Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

const MAX_MANIFEST: u64 = 1_000_000;
const MAX_MEMBER: u64 = 16_000_000;
const MAX_CAPTURE: u64 = 100_000_000;
const CASES: [(u64, u64, bool, usize); 4] = [
    (1344, 760, true, 4),
    (1344, 760, false, 1),
    (1928, 1208, true, 4),
    (1928, 1208, false, 1),
];

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn relative(name: &str) -> Result<&Path> {
    let path = Path::new(name);
    if name.is_empty()
        || !path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err("invalid driver original member path".into());
    }
    Ok(path)
}

fn read_regular(path: &Path, max: u64) -> Result<Vec<u8>> {
    let descriptor = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?;
    let mut file = File::from(descriptor);
    let before = file.metadata()?;
    if !before.is_file() || before.len() > max {
        return Err("driver original member is not bounded regular data".into());
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(max + 1)
        .read_to_end(&mut bytes)?;
    let held_after = file.metadata()?;
    let after = fs::symlink_metadata(path)?;
    if !after.is_file()
        || after.file_type().is_symlink()
        || bytes.len() as u64 != before.len()
        || before.dev() != held_after.dev()
        || before.ino() != held_after.ino()
        || before.len() != held_after.len()
        || before.mtime() != held_after.mtime()
        || before.mtime_nsec() != held_after.mtime_nsec()
        || before.ctime() != held_after.ctime()
        || before.ctime_nsec() != held_after.ctime_nsec()
        || before.dev() != after.dev()
        || before.ino() != after.ino()
        || before.mtime() != after.mtime()
        || before.mtime_nsec() != after.mtime_nsec()
        || before.ctime() != after.ctime()
        || before.ctime_nsec() != after.ctime_nsec()
    {
        return Err("driver original member changed during read".into());
    }
    Ok(bytes)
}

fn expected_roster() -> BTreeSet<String> {
    let mut roster = BTreeSet::from([String::from("report.json")]);
    for (width, _height, raw, frames) in CASES {
        let prefix = format!("{width}-raw-{}", usize::from(raw));
        for name in ["warmup.bin", "warmup-expected.bin", "report.json"] {
            roster.insert(format!("{prefix}/{name}"));
        }
        for index in 0..frames {
            roster.insert(format!("{prefix}/frame-{index}.bin"));
        }
        for index in 0..=frames {
            roster.insert(format!("{prefix}/reference/expected-{index}.bin"));
        }
        if raw {
            for name in ["calib-1.bin", "calib-3.bin", "invalid-calib.bin"] {
                roster.insert(format!("{prefix}/{name}"));
            }
        }
    }
    roster
}

pub struct OriginalCapture {
    root: PathBuf,
    manifest: Value,
    manifest_sha256: String,
    root_identity: (u64, u64),
}

impl OriginalCapture {
    /// Require the externally pinned original-only manifest and four-case roster.
    pub fn open(root: &Path, manifest_sha256: &str, source_commit: &str) -> Result<Self> {
        if manifest_sha256.len() != 64
            || !manifest_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            || source_commit.len() != 40
            || !source_commit.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("driver original identity pin is malformed".into());
        }
        let metadata = fs::symlink_metadata(root)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("driver original root must be a real directory".into());
        }
        let raw = read_regular(&root.join("reference-manifest.json"), MAX_MANIFEST)?;
        if sha(&raw) != manifest_sha256 {
            return Err("driver original manifest hash changed".into());
        }
        let manifest: Value = serde_json::from_slice(&raw)?;
        if manifest["source_commit"] != source_commit
            || manifest["policy"]["expected_packet_origin"]
                != "unchanged Python reference.get_driverstate_packet"
            || manifest["policy"]["candidate_message_files_are_not_oracle"] != true
        {
            return Err("driver capture provenance changed".into());
        }
        let cases = manifest["cases"].as_array().ok_or("missing driver cases")?;
        if cases.len() != CASES.len() {
            return Err("driver original case count changed".into());
        }
        for (case, (width, height, raw, frames)) in cases.iter().zip(CASES) {
            if case["name"] != format!("{width}-raw-{}", usize::from(raw))
                || case["width"] != width
                || case["height"] != height
                || case["raw"] != raw
                || case["frame_count"] != frames
                || case["reference_count"] != frames + 1
            {
                return Err("driver original case schedule changed".into());
            }
        }
        let files = manifest["files"]
            .as_object()
            .ok_or("missing driver files")?;
        if files.keys().cloned().collect::<BTreeSet<_>>() != expected_roster() {
            return Err("driver original input/reference roster changed".into());
        }
        let mut total = 0_u64;
        for (name, entry) in files {
            relative(name)?;
            for parent in Path::new(name).ancestors().skip(1) {
                if parent == Path::new("") {
                    break;
                }
                let metadata = fs::symlink_metadata(root.join(parent))?;
                if !metadata.is_dir() || metadata.file_type().is_symlink() {
                    return Err("driver original member parent is not a real directory".into());
                }
            }
            let len = entry["bytes"]
                .as_u64()
                .ok_or("missing original byte count")?;
            let expected = entry["sha256"].as_str().ok_or("missing original SHA")?;
            if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err("invalid driver original SHA".into());
            }
            let bytes = read_regular(&root.join(name), MAX_MEMBER)?;
            if bytes.len() as u64 != len || sha(&bytes) != expected {
                return Err(format!("driver original member changed: {name}").into());
            }
            total = total
                .checked_add(len)
                .ok_or("driver original size overflow")?;
        }
        if total > MAX_CAPTURE {
            return Err("driver original capture exceeds bound".into());
        }
        let report: Value =
            serde_json::from_slice(&read_regular(&root.join("report.json"), MAX_MEMBER)?)?;
        let runs = report["runs"]
            .as_array()
            .ok_or("missing original driver reports")?;
        if runs.len() != CASES.len() || report["device_validation"] != false {
            return Err("driver original top-level report changed".into());
        }
        for (run, (width, height, raw, frames)) in runs.iter().zip(CASES) {
            let prefix = format!("{width}-raw-{}", usize::from(raw));
            let case_report: Value = serde_json::from_slice(&read_regular(
                &root.join(format!("{prefix}/report.json")),
                MAX_MEMBER,
            )?)?;
            let logging = &run["logging"];
            let source_calls = &logging["source_calls"];
            if case_report != *run
                || run["camera"] != serde_json::json!([width, height])
                || run["raw_predictions"] != raw
                || run["frames"] != frames
                || run["compared_fields"] != frames * 47
                || run["first_frame_compared_fields"] != 47
                || run["queue_sizes"]
                    != serde_json::json!({"liveCalibration":256000,"driverStateV2":256000})
                || run["queue_size_oracle"] != "original cereal.services.SERVICE_LIST"
                || logging["result"] != "pass"
                || logging["component"] != "dmonitoringmodeld"
                || logging["log_messages"] != 5
                || logging["error_messages"] != 0
                || logging["runtime_timing_events"] != 0
                || logging["disk_records"] != 5
                || source_calls["result"] != "pass"
                || source_calls["compared_records"] != 5
                || source_calls["pending_egpu_lines"] != serde_json::json!([])
                || source_calls["compared_lines"]
                    .as_array()
                    .is_none_or(|lines| lines.len() != 4)
            {
                return Err("driver original report contract changed".into());
            }
            if read_regular(
                &root.join(format!("{prefix}/reference/expected-0.bin")),
                MAX_MEMBER,
            )? != read_regular(
                &root.join(format!("{prefix}/warmup-expected.bin")),
                MAX_MEMBER,
            )? {
                return Err("driver warmup oracle differs from unchanged checker".into());
            }
        }
        let capture = Self {
            root: root.into(),
            manifest,
            manifest_sha256: manifest_sha256.into(),
            root_identity: (metadata.dev(), metadata.ino()),
        };
        capture.loaded_python_sources()?;
        capture.original_models()?;
        Ok(capture)
    }

    /// Rehash an inventoried input or source-computed expected packet on use.
    pub fn read(&self, name: &str) -> Result<Vec<u8>> {
        relative(name)?;
        for parent in Path::new(name).ancestors().skip(1) {
            if parent == Path::new("") {
                break;
            }
            let metadata = fs::symlink_metadata(self.root.join(parent))?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err("driver original member parent changed".into());
            }
        }
        let entry = self.manifest["files"]
            .get(name)
            .ok_or("driver original member is not inventoried")?;
        let bytes = read_regular(&self.root.join(name), MAX_MEMBER)?;
        if entry["bytes"].as_u64() != Some(bytes.len() as u64)
            || entry["sha256"].as_str() != Some(sha(&bytes).as_str())
        {
            return Err(format!("driver original member changed: {name}").into());
        }
        Ok(bytes)
    }

    /// Recheck the original-only capture after all actual IPC and output work.
    pub fn verify(&self) -> Result<()> {
        let root = fs::symlink_metadata(&self.root)?;
        if !root.is_dir()
            || root.file_type().is_symlink()
            || (root.dev(), root.ino()) != self.root_identity
            || sha(&read_regular(
                &self.root.join("reference-manifest.json"),
                MAX_MANIFEST,
            )?) != self.manifest_sha256
        {
            return Err("driver original root or manifest changed".into());
        }
        for name in expected_roster() {
            self.read(&name)?;
        }
        Ok(())
    }

    fn source_inputs(&self) -> Result<Vec<(&str, &str)>> {
        let inputs = self.manifest["original_source_inputs"]
            .as_object()
            .ok_or("original driver source inventory missing")?;
        if inputs.len() != 15 {
            return Err("original driver source inventory count changed".into());
        }
        inputs
            .iter()
            .map(|(name, entry)| {
                relative(name)?;
                let hash = entry["sha256"]
                    .as_str()
                    .ok_or("original source digest missing")?;
                if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return Err("original source digest malformed".into());
                }
                Ok((name.as_str(), hash))
            })
            .collect()
    }

    fn loaded_python_sources(&self) -> Result<Vec<(&str, &str)>> {
        let loaded = self.manifest["loaded_python_sources"]
            .as_object()
            .ok_or("original imported Python inventory missing")?;
        if loaded.is_empty() || loaded.len() > 2_000 {
            return Err("original imported Python inventory count changed".into());
        }
        let mut total = 0_u64;
        loaded
            .iter()
            .map(|(name, entry)| {
                relative(name)?;
                if !name.ends_with(".py") {
                    return Err("imported source is not Python".into());
                }
                let bytes = entry["bytes"]
                    .as_u64()
                    .ok_or("imported source byte count missing")?;
                total = total
                    .checked_add(bytes)
                    .ok_or("imported source size overflow")?;
                if bytes > 4_000_000 || total > 256_000_000 {
                    return Err("imported source inventory exceeds bound".into());
                }
                let hash = entry["sha256"]
                    .as_str()
                    .ok_or("imported source digest missing")?;
                if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return Err("imported source digest malformed".into());
                }
                Ok((name.as_str(), hash))
            })
            .collect()
    }

    fn original_models(&self) -> Result<Vec<(&str, &str)>> {
        let models = self.manifest["original_models"]
            .as_object()
            .ok_or("original driver model inventory missing")?;
        let expected = BTreeSet::from([
            "dmonitoring_model_tinygrad.pkl",
            "dm_warp_1344x760_tinygrad.pkl",
            "dm_warp_1928x1208_tinygrad.pkl",
            "dmonitoring_model_metadata.pkl",
        ]);
        if models.keys().map(String::as_str).collect::<BTreeSet<_>>() != expected {
            return Err("original driver model roster changed".into());
        }
        models
            .iter()
            .map(|(name, entry)| {
                let hash = entry["sha256"]
                    .as_str()
                    .ok_or("original model digest missing")?;
                if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return Err("original model digest malformed".into());
                }
                Ok((name.as_str(), hash))
            })
            .collect()
    }
}

fn compare_value(expected: &Value, observed: &Value, path: &str) -> Result<usize> {
    match (expected, observed) {
        (Value::Object(a), Value::Object(b))
            if a.len() == 1
                && b.len() == 1
                && a.contains_key("float64_bits")
                && b.contains_key("float64_bits") =>
        {
            let parse = |value: &Value| -> Result<f64> {
                Ok(f64::from_bits(u64::from_str_radix(
                    value
                        .as_str()
                        .ok_or("invalid driver nonfinite float bits")?,
                    16,
                )?))
            };
            let left = parse(&a["float64_bits"])?;
            let right = parse(&b["float64_bits"])?;
            if left == right || (left.is_nan() && right.is_nan()) {
                Ok(1)
            } else {
                Err(format!("{path}: nonfinite value changed").into())
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            if a.keys().collect::<Vec<_>>() != b.keys().collect::<Vec<_>>() {
                return Err(format!("{path}: original driver packet keys changed").into());
            }
            a.iter().try_fold(0, |count, (name, value)| {
                Ok(count + compare_value(value, &b[name], &format!("{path}.{name}"))?)
            })
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Err(format!("{path}: original driver array length changed").into());
            }
            a.iter()
                .zip(b)
                .enumerate()
                .try_fold(0, |count, (index, (left, right))| {
                    Ok(count + compare_value(left, right, &format!("{path}[{index}]"))?)
                })
        }
        (Value::Number(a), Value::Number(b)) if a.is_f64() => {
            let left = a.as_f64().ok_or("original driver float missing")?;
            let right = b.as_f64().ok_or("native driver float missing")?;
            let tolerance = if path.contains("Coefficients") {
                2e-5
            } else {
                1e-6
            };
            if !(left == right
                || (left.is_nan() && right.is_nan())
                || (left.is_finite()
                    && right.is_finite()
                    && (left - right).abs() <= tolerance * left.abs().max(right.abs()).max(1.0)))
            {
                return Err(format!("{path}: original driver float changed").into());
            }
            Ok(1)
        }
        _ if expected == observed => Ok(1),
        _ => Err(format!("{path}: original driver field changed").into()),
    }
}

pub struct PacketComparison {
    pub fields: usize,
    pub log_mono_time: u64,
    pub model_execution_time: f64,
    pub gpu_execution_time: f64,
}

/// Compare one actual Cereal publication to source-computed Python bytes.
/// Only the observed clock and two process timings vary between executions.
pub fn compare_driver_packet(
    source: &[u8],
    candidate: &[u8],
    sent_ns: u64,
    received_ns: u64,
) -> Result<PacketComparison> {
    let mut expected = event_dict(source)?;
    let mut actual = event_dict(candidate)?;
    let expected_root = expected
        .as_object_mut()
        .ok_or("original Event is not an object")?;
    let actual_root = actual
        .as_object_mut()
        .ok_or("native Event is not an object")?;
    let stamp = actual_root
        .remove("logMonoTime")
        .and_then(|value| value.as_u64())
        .ok_or("native driver clock missing")?;
    if !(sent_ns..=received_ns).contains(&stamp) {
        return Err("driver publication clock outside send/receive interval".into());
    }
    expected_root
        .remove("logMonoTime")
        .ok_or("original driver clock missing")?;
    let dynamic = |root: &mut serde_json::Map<String, Value>, name: &str| -> Result<f64> {
        root.get_mut("driverStateV2")
            .and_then(Value::as_object_mut)
            .and_then(|row| row.remove(name))
            .and_then(|value| value.as_f64())
            .ok_or_else(|| format!("driver {name} missing").into())
    };
    dynamic(expected_root, "modelExecutionTime")?;
    dynamic(expected_root, "gpuExecutionTime")?;
    let model = dynamic(actual_root, "modelExecutionTime")?;
    let gpu = dynamic(actual_root, "gpuExecutionTime")?;
    if !(0.0 < gpu && gpu <= model && model < 30.0) {
        return Err("driver timings outside original bounds".into());
    }
    let fields = compare_value(&expected, &actual, "driverStateV2")? + 3;
    Ok(PacketComparison {
        fields,
        log_mono_time: stamp,
        model_execution_time: model,
        gpu_execution_time: gpu,
    })
}

const LINE_WAIT: Duration = Duration::from_secs(60);
const SHUTDOWN: Duration = Duration::from_secs(5);
const LOG_LIMIT: usize = 16 * 1024 * 1024;

/// Four-case native replay. The peer and original-only capture are supplied by
/// the hosted prerequisite; this controller never executes the Python checker.
pub struct Options {
    pub binary: PathBuf,
    pub catalog: PathBuf,
    pub collector: PathBuf,
    pub peer: PathBuf,
    pub models: PathBuf,
    pub original_dir: PathBuf,
    pub original_manifest_sha256: String,
    pub source_commit: String,
    pub output: PathBuf,
    pub root: PathBuf,
}

struct InputPin {
    path: PathBuf,
    file: File,
    identity: (u64, u64, u64, i64, i64, i64, i64),
    digest: String,
}
struct CatalogPin {
    root: PathBuf,
    identity: (u64, u64),
    files: Vec<InputPin>,
    names: BTreeSet<PathBuf>,
}
impl CatalogPin {
    fn list(root: &Path) -> Result<BTreeSet<PathBuf>> {
        fn visit(root: &Path, directory: &Path, names: &mut BTreeSet<PathBuf>) -> Result<()> {
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                let path = entry.path();
                let kind = entry.file_type()?;
                if kind.is_symlink() {
                    return Err("driver catalog contains symlink".into());
                }
                if kind.is_dir() {
                    visit(root, &path, names)?;
                } else if kind.is_file() {
                    names.insert(path.strip_prefix(root)?.to_owned());
                } else {
                    return Err("driver catalog contains special file".into());
                }
                if names.len() > 2_000 {
                    return Err("driver catalog exceeds file count bound".into());
                }
            }
            Ok(())
        }
        let mut names = BTreeSet::new();
        visit(root, root, &mut names)?;
        if !names.contains(Path::new("current.json")) || names.len() < 3 {
            return Err("driver catalog lacks generation or pointer".into());
        }
        Ok(names)
    }
    fn open(root: &Path) -> Result<Self> {
        let metadata = fs::symlink_metadata(root)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("driver catalog root must be a real directory".into());
        }
        let names = Self::list(root)?;
        let mut files = Vec::new();
        let mut total = 0_u64;
        for name in &names {
            let pin = InputPin::open(&root.join(name), false)?;
            total = total
                .checked_add(pin.identity.2)
                .ok_or("driver catalog size overflow")?;
            if total > 2_000_000_000 {
                return Err("driver catalog exceeds size bound".into());
            }
            files.push(pin);
        }
        let catalog = Self {
            root: root.into(),
            identity: (metadata.dev(), metadata.ino()),
            files,
            names,
        };
        catalog.verify()?;
        Ok(catalog)
    }
    fn verify(&self) -> Result<()> {
        let metadata = fs::symlink_metadata(&self.root)?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || self.identity != (metadata.dev(), metadata.ino())
            || self.names != Self::list(&self.root)?
        {
            return Err("driver catalog root or roster changed".into());
        }
        for file in &self.files {
            file.verify()?;
        }
        Ok(())
    }
    fn receipt(&self) -> Value {
        json!({"root":self.root,"files":self.files.iter().map(InputPin::receipt).collect::<Vec<_>>()})
    }
}

fn verify_catalog_sources(catalog: &CatalogPin, models: &BTreeMap<String, String>) -> Result<()> {
    let pointer: Value = serde_json::from_slice(&read_regular(
        &catalog.root.join("current.json"),
        MAX_MANIFEST,
    )?)?;
    let generation = pointer["generation"]
        .as_str()
        .ok_or("driver catalog generation missing")?;
    if pointer["version"] != 1
        || generation.len() != 64
        || !generation.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("driver catalog pointer changed".into());
    }
    let index_name = format!("{generation}/index.json");
    if !catalog.names.contains(Path::new(&index_name)) {
        return Err("driver catalog index missing".into());
    }
    let index: Value =
        serde_json::from_slice(&read_regular(&catalog.root.join(index_name), MAX_MANIFEST)?)?;
    let bundles = index["bundles"]
        .as_array()
        .filter(|bundles| index["version"] == 1 && !bundles.is_empty() && bundles.len() <= 16)
        .ok_or("driver catalog index invalid")?;
    for (width, height, _, _) in [CASES[0], CASES[2]] {
        let matches = bundles
            .iter()
            .filter(|bundle| {
                bundle["kind"] == "driver" && bundle["camera"] == json!([width, height])
            })
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err("driver catalog lacks unique camera bundle".into());
        }
        let directory = matches[0]["directory"]
            .as_str()
            .filter(|directory| {
                !directory.is_empty()
                    && Path::new(directory)
                        .components()
                        .all(|part| matches!(part, std::path::Component::Normal(_)))
            })
            .ok_or("driver catalog bundle directory invalid")?;
        let name = format!("{generation}/{directory}/pipeline.json");
        if !catalog.names.contains(Path::new(&name)) {
            return Err("driver catalog pipeline missing".into());
        }
        let descriptor: Value =
            serde_json::from_slice(&read_regular(&catalog.root.join(name), MAX_MEMBER)?)?;
        let warp = format!("dm_warp_{width}x{height}_tinygrad.pkl");
        let expected = json!({
            "model":models.get("dmonitoring_model_tinygrad.pkl").ok_or("driver model pin missing")?,
            "metadata":models.get("dmonitoring_model_metadata.pkl").ok_or("driver metadata pin missing")?,
            "warp":models.get(&warp).ok_or("driver warp pin missing")?,
        });
        if descriptor["version"] != 1
            || descriptor["kind"] != "driver"
            || descriptor["camera"] != json!([width, height])
            || descriptor["sources"] != expected
        {
            return Err(
                format!("driver {width}x{height} catalog source/model PKL link changed").into(),
            );
        }
    }
    Ok(())
}
impl InputPin {
    fn open(path: &Path, elf: bool) -> Result<Self> {
        let descriptor = rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        let mut file = File::from(descriptor);
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() > 512_000_000 {
            return Err("driver input is not a bounded regular file".into());
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(512_000_001)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != metadata.len() {
            return Err("driver input changed while binding".into());
        }
        if elf {
            let arch = if cfg!(target_arch = "aarch64") {
                183
            } else {
                62
            };
            if bytes.len() < 20
                || &bytes[..6] != b"\x7fELF\x02\x01"
                || u16::from_le_bytes([bytes[18], bytes[19]]) != arch
            {
                return Err("driver executable is not a native host ELF".into());
            }
        }
        let identity = file_identity(&metadata);
        let pin = Self {
            path: path.to_owned(),
            file,
            identity,
            digest: sha(&bytes),
        };
        pin.verify()?;
        Ok(pin)
    }
    fn verify(&self) -> Result<()> {
        let by_fd = self.file.metadata()?;
        let by_path = fs::symlink_metadata(&self.path)?;
        if by_path.file_type().is_symlink()
            || !by_path.is_file()
            || self.identity != file_identity(&by_fd)
            || self.identity != file_identity(&by_path)
            || self.digest != sha(&read_regular(&self.path, 512_000_000)?)
        {
            return Err(format!("driver input changed: {}", self.path.display()).into());
        }
        Ok(())
    }
    fn receipt(&self) -> Value {
        json!({"path":self.path,"sha256":self.digest,"dev":self.identity.0,
            "ino":self.identity.1,"bytes":self.identity.2})
    }
}
fn file_identity(m: &fs::Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
    (
        m.dev(),
        m.ino(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}

fn real_parent_chain(root: &Path, name: &str) -> Result<()> {
    relative(name)?;
    for parent in Path::new(name).ancestors().skip(1) {
        if parent == Path::new("") {
            break;
        }
        let metadata = fs::symlink_metadata(root.join(parent))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("driver imported source parent is not a real directory".into());
        }
    }
    Ok(())
}

fn checkout_commit(root: &Path) -> Result<String> {
    let mut spec = CommandSpec::new_os(
        vec![
            "/usr/bin/git".into(),
            "-C".into(),
            root.as_os_str().to_owned(),
            "rev-parse".into(),
            "HEAD".into(),
        ],
        Duration::from_secs(10),
    );
    spec.stdout_limit = 128;
    spec.stderr_limit = 4096;
    let outcome = NativeRunner.run(&spec)?;
    if outcome.code != 0 {
        return Err("driver source checkout cannot be identified".into());
    }
    let head = String::from_utf8(outcome.stdout)?.trim().to_owned();
    if head.len() != 40 || !head.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("driver source checkout identity is malformed".into());
    }
    Ok(head)
}

struct Signals {
    cancelled: Arc<AtomicBool>,
    registrations: Vec<signal_hook::SigId>,
}
impl Signals {
    fn new() -> Result<Self> {
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut registrations = Vec::new();
        for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            registrations.push(signal_hook::flag::register(signal, Arc::clone(&cancelled))?);
        }
        Ok(Self {
            cancelled,
            registrations,
        })
    }
    fn check(&self) -> Result<()> {
        if self.cancelled.load(Ordering::Relaxed) {
            Err("driver controller cancelled".into())
        } else {
            Ok(())
        }
    }
}
impl Drop for Signals {
    fn drop(&mut self) {
        for id in self.registrations.drain(..) {
            signal_hook::low_level::unregister(id);
        }
    }
}

fn now_ns() -> Result<u64> {
    let now = clock_gettime(ClockId::Monotonic);
    Ok(u64::try_from(now.tv_sec)? * 1_000_000_000 + u64::try_from(now.tv_nsec)?)
}

struct Lines {
    pipe: File,
    log: File,
    pending: Vec<u8>,
    total: usize,
}
impl Lines {
    fn until(&mut self, tokens: &[&str], signals: &Signals) -> Result<String> {
        self.until_with_poll(tokens, signals, |pipe, timeout| {
            poll(&mut [PollFd::new(pipe, PollFlags::IN)], Some(timeout))
        })
    }
    fn until_with_poll(
        &mut self,
        tokens: &[&str],
        signals: &Signals,
        mut wait: impl FnMut(&File, &Timespec) -> rustix::io::Result<usize>,
    ) -> Result<String> {
        let started = Instant::now();
        loop {
            signals.check()?;
            while let Some(index) = self.pending.iter().position(|byte| *byte == b'\n') {
                let line = self.pending.drain(..=index).collect::<Vec<_>>();
                let text = String::from_utf8(line)?.trim_end().to_owned();
                if tokens.iter().any(|token| text.contains(token)) {
                    return Ok(text);
                }
            }
            if started.elapsed() >= LINE_WAIT {
                return Err("driver child line exceeded 60 seconds".into());
            }
            let timeout = Timespec::try_from(Duration::from_millis(20))?;
            match wait(&self.pipe, &timeout) {
                Ok(_) | Err(rustix::io::Errno::INTR) => {}
                Err(error) => return Err(error.into()),
            }
            let mut chunk = [0; 4096];
            match self.pipe.read(&mut chunk) {
                Ok(0) => return Err("driver child EOF before expected line".into()),
                Ok(count) => {
                    self.total = self
                        .total
                        .checked_add(count)
                        .ok_or("driver log length overflow")?;
                    if self.total > LOG_LIMIT {
                        return Err("driver child log exceeds 16 MiB".into());
                    }
                    self.log.write_all(&chunk[..count])?;
                    self.pending.extend_from_slice(&chunk[..count]);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
}

struct OwnedChild {
    child: Child,
    lines: Lines,
    reaped: bool,
}
impl OwnedChild {
    fn spawn(
        program: &Path,
        args: &[OsString],
        env: &[(OsString, OsString)],
        log: &Path,
        peer: bool,
    ) -> Result<Self> {
        let mut command = Command::new(program);
        command
            .env_remove("SIMULATION")
            .env_remove("SEND_RAW_PRED")
            .args(args)
            .envs(env.iter().cloned())
            .process_group(0)
            .stdin(if peer { Stdio::piped() } else { Stdio::null() })
            .stdout(if peer { Stdio::piped() } else { Stdio::null() })
            .stderr(if peer {
                Stdio::from(File::create(log.with_extension("stderr"))?)
            } else {
                Stdio::piped()
            });
        let mut child = command.spawn()?;
        let pipe: OwnedFd = if peer {
            child
                .stdout
                .take()
                .ok_or("driver peer stdout missing")?
                .into()
        } else {
            child
                .stderr
                .take()
                .ok_or("driver daemon stderr missing")?
                .into()
        };
        let flags = rustix::fs::fcntl_getfl(&pipe)?;
        rustix::fs::fcntl_setfl(&pipe, flags | rustix::fs::OFlags::NONBLOCK)?;
        Ok(Self {
            child,
            lines: Lines {
                pipe: File::from(pipe),
                log: File::create(log)?,
                pending: Vec::new(),
                total: 0,
            },
            reaped: false,
        })
    }
    fn wait_line(&mut self, tokens: &[&str], signals: &Signals) -> Result<String> {
        self.lines.until(tokens, signals)
    }
    fn command(&mut self, value: &str, signals: &Signals) -> Result<String> {
        let input = self
            .child
            .stdin
            .as_mut()
            .ok_or("driver peer stdin missing")?;
        input.write_all(value.as_bytes())?;
        input.write_all(b"\n")?;
        input.flush()?;
        self.wait_line(&["OK", "TIMEOUT"], signals)
    }
    fn alive(&mut self) -> Result<()> {
        if let Some(status) = self.child.try_wait()? {
            self.reaped = true;
            let _ = kill_process_group(Pid::from_child(&self.child), Signal::KILL);
            return Err(format!("driver daemon exited while waiting: {status}").into());
        }
        Ok(())
    }
    fn stop(
        &mut self,
        signal: Option<Signal>,
        expected_code: i32,
        timeout: Duration,
        signals: &Signals,
    ) -> Result<()> {
        signals.check()?;
        if let Some(signal) = signal {
            kill_process(Pid::from_child(&self.child), signal)?;
        }
        let started = Instant::now();
        loop {
            signals.check()?;
            if let Some(status) = self.child.try_wait()? {
                self.reaped = true;
                let _ = kill_process_group(Pid::from_child(&self.child), Signal::KILL);
                if status.code() == Some(expected_code) {
                    return Ok(());
                }
                return Err(format!("driver child exit changed: {status}").into());
            }
            if started.elapsed() >= timeout {
                return Err("driver child shutdown deadline exceeded".into());
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = kill_process_group(Pid::from_child(&self.child), Signal::KILL);
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn protocol_path(path: &Path) -> Result<&str> {
    let text = path.to_str().ok_or("driver peer path is not UTF-8")?;
    if text.chars().any(char::is_whitespace) {
        return Err("driver peer path contains whitespace".into());
    }
    Ok(text)
}
fn reply(actual: String, wanted: &str) -> Result<()> {
    if actual == wanted {
        Ok(())
    } else {
        Err(format!("driver peer returned {actual}, expected {wanted}").into())
    }
}
fn send(peer: &mut OwnedChild, command: &str, signals: &Signals) -> Result<()> {
    reply(peer.command(command, signals)?, "OK")
}
fn receive(
    peer: &mut OwnedChild,
    timeout: u32,
    path: &Path,
    want: &str,
    signals: &Signals,
) -> Result<()> {
    reply(
        peer.command(
            &format!("receive {timeout} {}", protocol_path(path)?),
            signals,
        )?,
        want,
    )
}
fn seal_member(original: &OriginalCapture, name: &str, destination: &Path) -> Result<()> {
    let bytes = original.read(name)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    file.write_all(&bytes)?;
    file.flush()?;
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.flush()?;
    Ok(())
}

struct TransportNamespace {
    shm: PathBuf,
    prefix: String,
    finished: bool,
}
impl TransportNamespace {
    fn sockets(prefix: &str, unlink: bool) -> Result<usize> {
        let mut count = 0_usize;
        let mut foreign = false;
        let marker = format!("{prefix}_visionipc_");
        for entry in fs::read_dir("/tmp")? {
            let entry = entry?;
            if !entry.file_name().as_bytes().starts_with(marker.as_bytes()) {
                continue;
            }
            count += 1;
            if count > 128 {
                return Err("driver socket namespace exceeds bound".into());
            }
            let kind = entry.file_type()?;
            if !kind.is_socket() {
                foreign = true;
                continue;
            }
            if unlink {
                fs::remove_file(entry.path())?;
            }
        }
        if foreign {
            return Err("driver socket namespace contains foreign node".into());
        }
        Ok(count)
    }
    fn new(prefix: String) -> Result<Self> {
        if Self::sockets(&prefix, false)? != 0 {
            return Err("driver socket namespace existed before owned case".into());
        }
        let shm = Path::new("/dev/shm").join(format!("msgq_{prefix}"));
        fs::create_dir(&shm)?;
        Ok(Self {
            shm,
            prefix,
            finished: false,
        })
    }
    fn finish(&mut self) -> Result<()> {
        Self::sockets(&self.prefix, true)?;
        if Self::sockets(&self.prefix, false)? != 0 {
            return Err("driver IPC socket cleanup incomplete".into());
        }
        fs::remove_dir_all(&self.shm)?;
        if self.shm.exists() {
            return Err("driver shared memory cleanup incomplete".into());
        }
        self.finished = true;
        Ok(())
    }
}
impl Drop for TransportNamespace {
    fn drop(&mut self) {
        if !self.finished {
            let _ = Self::sockets(&self.prefix, true);
            let _ = fs::remove_dir_all(&self.shm);
        }
    }
}

fn source_calls(
    records: &[Value],
    daemon_pid: u32,
    waiting_pid: u32,
    size: u64,
    expected: &Value,
) -> Result<Value> {
    if records.len() != 5 {
        return Err("driver collector record count changed".into());
    }
    let daemon = records
        .iter()
        .filter(|row| row["process"] == daemon_pid)
        .collect::<Vec<_>>();
    let waiting = records
        .iter()
        .filter(|row| row["process"] == waiting_pid)
        .collect::<Vec<_>>();
    let wanted_daemon = [
        "connecting to driver stream".to_owned(),
        format!("connected with buffer size: {size}"),
        "models loaded, dmonitoringmodeld starting".to_owned(),
    ];
    let wanted_waiting = ["connecting to driver stream", "got SIGINT"];
    if daemon.len() != wanted_daemon.len()
        || waiting.len() != wanted_waiting.len()
        || !daemon
            .iter()
            .zip(wanted_daemon)
            .all(|(row, text)| row["levelnum"] == 30 && row["msg"] == text)
        || !waiting
            .iter()
            .zip(wanted_waiting)
            .all(|(row, text)| row["levelnum"] == 30 && row["msg"] == text)
    {
        return Err("driver source-call messages, order, or process roles changed".into());
    }
    let lines = &expected["compared_lines"];
    if !lines
        .as_array()
        .is_some_and(|values| values.len() == 4 && values.iter().all(Value::is_u64))
        || expected["pending_egpu_lines"] != json!([])
        || expected["compared_records"] != 5
        || expected["result"] != "pass"
    {
        return Err("independent original source-call report changed".into());
    }
    Ok(json!({"result":"pass","compared_records":5,"compared_lines":lines,"pending_egpu_lines":[]}))
}

struct CaseContext<'a> {
    options: &'a Options,
    original: &'a OriginalCapture,
    signals: &'a Signals,
    width: u64,
    height: u64,
    raw: bool,
}

fn case(context: &CaseContext<'_>) -> Result<Value> {
    let CaseContext {
        options,
        original,
        signals,
        width,
        height,
        raw,
    } = *context;
    signals.check()?;
    let prefix = format!("{width}-raw-{}", usize::from(raw));
    let destination = options.output.join(&prefix);
    fs::create_dir(&destination)?;
    let shm_prefix = format!(
        "driver-native-{}-{width}-{}",
        std::process::id(),
        usize::from(raw)
    );
    let mut namespace = TransportNamespace::new(shm_prefix.clone())?;
    let shm = namespace.shm.clone();
    let env = [(
        OsString::from("OPENPILOT_PREFIX"),
        OsString::from(&shm_prefix),
    )]
    .into_iter()
    .chain(raw.then_some((OsString::from("SEND_RAW_PRED"), OsString::from("0"))))
    .collect::<Vec<_>>();
    let mut collector = Collector::start(
        &options.collector,
        &destination.join("logging"),
        &env,
        &shm_prefix,
        &options.root,
        &options.source_commit,
    )?;
    let daemon_args = [
        OsString::from("--trusted-catalog"),
        options.catalog.as_os_str().to_owned(),
    ];
    let mut daemon = OwnedChild::spawn(
        &options.binary,
        &daemon_args,
        &env,
        &destination.join("daemon.log"),
        false,
    )?;
    let daemon_pid = daemon.child.id();
    daemon.wait_line(&["connecting to driver stream"], signals)?;
    thread::sleep(Duration::from_millis(150));
    signals.check()?;
    daemon.alive()?;
    let stride = width.div_ceil(128) * 128;
    let y_height = height.div_ceil(32) * 32;
    let uv_height = (height / 2).div_ceil(16) * 16;
    let mut size = stride * y_height + stride * uv_height + 4096;
    size += (16 * 1024).max(8 * stride);
    size = size.div_ceil(4096) * 4096;
    size += width.div_ceil(512) * 512 * 512;
    size = size.div_ceil(4096) * 4096;
    let peer_args = [width, height, stride, stride * y_height, size]
        .into_iter()
        .map(|n| OsString::from(n.to_string()))
        .collect::<Vec<_>>();
    let mut peer = OwnedChild::spawn(
        &options.peer,
        &peer_args,
        &env,
        &destination.join("peer.log"),
        true,
    )?;
    let ready = peer.wait_line(&["READY"], signals)?;
    let header_bytes = ready
        .strip_prefix("READY ")
        .ok_or("driver peer READY line changed")?
        .parse::<u64>()?;
    daemon.wait_line(&["connected with buffer size"], signals)?;
    daemon.wait_line(&["models loaded"], signals)?;
    let unexpected = destination.join("unexpected.bin");
    receive(&mut peer, 200, &unexpected, "TIMEOUT", signals)?;
    if unexpected.exists() {
        return Err("driver emitted before first frame".into());
    }
    let warmup = destination.join("warmup.bin");
    seal_member(original, &format!("{prefix}/warmup.bin"), &warmup)?;
    if fs::metadata(&warmup)?.len() != size {
        return Err("driver warmup image size changed".into());
    }
    let warmup_start = now_ns()?;
    send(
        &mut peer,
        &format!("send 10 {}", protocol_path(&warmup)?),
        signals,
    )?;
    let mut queue_sizes = serde_json::Map::new();
    for topic in ["liveCalibration", "driverStateV2"] {
        let length = fs::metadata(shm.join(topic))?.len();
        let capacity = length
            .checked_sub(header_bytes)
            .ok_or("driver queue header exceeds file")?;
        if capacity != 256_000 {
            return Err("driver original service queue capacity changed".into());
        }
        queue_sizes.insert(topic.into(), json!(capacity));
    }
    let warmup_packet = destination.join("warmup-message.bin");
    receive(&mut peer, 30_000, &warmup_packet, "OK", signals)?;
    let warmup_comparison = compare_driver_packet(
        &original.read(&format!("{prefix}/reference/expected-0.bin"))?,
        &read_regular(&warmup_packet, 4_000_000)?,
        warmup_start,
        now_ns()?,
    )?;
    if warmup_comparison.fields != 47 {
        return Err("driver warmup comparison field count changed".into());
    }
    receive(&mut peer, 200, &unexpected, "TIMEOUT", signals)?;
    if unexpected.exists() {
        return Err("driver emitted duplicate warmup frame".into());
    }
    let frames = if raw { 4 } else { 1 };
    let mut fields = 0_usize;
    let mut previous_stamp = 0_u64;
    let mut packets = Vec::new();
    for index in 0..frames {
        signals.check()?;
        if raw && (index == 1 || index == 3) {
            let path = destination.join(format!("calib-{index}.bin"));
            seal_member(original, &format!("{prefix}/calib-{index}.bin"), &path)?;
            send(
                &mut peer,
                &format!("calib {}", protocol_path(&path)?),
                signals,
            )?;
        }
        let path = destination.join(format!("frame-{index}.bin"));
        seal_member(original, &format!("{prefix}/frame-{index}.bin"), &path)?;
        if fs::metadata(&path)?.len() != size {
            return Err("driver frame size changed".into());
        }
        let start = now_ns()?;
        send(
            &mut peer,
            &format!("send {} {}", index + 11, protocol_path(&path)?),
            signals,
        )?;
        let packet_path = destination.join(format!("message-{index}.bin"));
        receive(&mut peer, 30_000, &packet_path, "OK", signals)?;
        let end = now_ns()?;
        let candidate = read_regular(&packet_path, 4_000_000)?;
        let comparison = compare_driver_packet(
            &original.read(&format!("{prefix}/reference/expected-{}.bin", index + 1))?,
            &candidate,
            start,
            end,
        )?;
        if comparison.fields != 47 || comparison.log_mono_time <= previous_stamp {
            return Err("driver frame field count or clock sequence changed".into());
        }
        previous_stamp = comparison.log_mono_time;
        fields += comparison.fields;
        packets.push(json!({"frame_id":index+11,"bytes":candidate.len(),
            "sha256":sha(&candidate),"log_mono_time":comparison.log_mono_time,
            "model_execution_time":comparison.model_execution_time,
            "gpu_execution_time":comparison.gpu_execution_time}));
        receive(&mut peer, 200, &unexpected, "TIMEOUT", signals)?;
        if unexpected.exists() {
            return Err("driver emitted duplicate frame".into());
        }
    }
    if raw {
        let invalid = destination.join("invalid-calib.bin");
        seal_member(original, &format!("{prefix}/invalid-calib.bin"), &invalid)?;
        send(
            &mut peer,
            &format!("calib {}", protocol_path(&invalid)?),
            signals,
        )?;
        let first = destination.join("frame-0.bin");
        send(
            &mut peer,
            &format!("send 90 {}", protocol_path(&first)?),
            signals,
        )?;
        daemon.stop(None, 1, Duration::from_secs(10), signals)?;
        daemon.wait_line(&["calibration must broadcast"], signals)?;
    } else {
        daemon.stop(Some(Signal::TERM), 0, SHUTDOWN, signals)?;
    }
    send(&mut peer, "stop", signals)?;
    peer.stop(None, 0, SHUTDOWN, signals)?;
    let mut waiting = OwnedChild::spawn(
        &options.binary,
        &daemon_args,
        &env,
        &destination.join("waiting.log"),
        false,
    )?;
    let waiting_pid = waiting.child.id();
    waiting.wait_line(&["connecting to driver stream"], signals)?;
    waiting.stop(Some(Signal::INT), 0, SHUTDOWN, signals)?;
    let mut logging = collector.finish_driver(&[daemon_pid, peer.child.id(), waiting_pid])?;
    let original_report: Value =
        serde_json::from_slice(&original.read(&format!("{prefix}/report.json"))?)?;
    let records = collector
        .records("logMessage")
        .ok_or("driver logMessage collector missing")?;
    logging["source_calls"] = source_calls(
        records,
        daemon_pid,
        waiting_pid,
        size,
        &original_report["logging"]["source_calls"],
    )?;
    for (name, expected) in [
        ("log_messages", 5),
        ("error_messages", 0),
        ("runtime_timing_events", 0),
        ("disk_records", 5),
    ] {
        if logging[name] != expected {
            return Err(format!("driver collector {name} changed").into());
        }
    }
    let mut scenarios = vec![
        "original service queue capacities",
        "delayed camera",
        "frame identity",
        "camera validity ignored as original",
        "zero calibration",
        "no-frame timeout",
        "SIGINT during connect",
    ];
    if raw {
        scenarios.extend([
            "invalid-flag calibration update",
            "retained calibration",
            "broadcast calibration",
            "malformed calibration exit",
        ]);
    } else {
        scenarios.push("SIGTERM during receive");
    }
    let report = json!({"camera":[width,height],"frames":frames,"raw_predictions":raw,
        "compared_fields":fields,"queue_sizes":queue_sizes,
        "queue_size_oracle":"original cereal.services.SERVICE_LIST",
        "first_frame_id":10,"first_frame_compared_fields":warmup_comparison.fields,
        "model_loaded_before_first_frame":true,"raw_comparison":"exact bytes",
        "parsed_float_tolerance":1e-6,"device_validation":false,
        "logging":logging,"packets":packets,"scenarios":scenarios});
    for key in [
        "camera",
        "frames",
        "raw_predictions",
        "compared_fields",
        "queue_sizes",
        "queue_size_oracle",
        "first_frame_id",
        "first_frame_compared_fields",
        "model_loaded_before_first_frame",
        "raw_comparison",
        "parsed_float_tolerance",
        "device_validation",
        "scenarios",
    ] {
        if report[key] != original_report[key] {
            return Err(format!("driver independent report field {key} changed").into());
        }
    }
    signals.check()?;
    original.verify()?;
    namespace.finish()?;
    write_new(
        &destination.join("report.json"),
        format!("{}\n", serde_json::to_string_pretty(&report)?).as_bytes(),
    )?;
    Ok(report)
}

/// Run all four original input cases against one sealed native daemon/peer pair.
/// The output directory is exclusive and a success receipt appears only last.
pub fn run(options: &Options) -> Result<Value> {
    let signals = Signals::new()?;
    if [
        &options.output,
        &options.root,
        &options.binary,
        &options.catalog,
        &options.collector,
        &options.peer,
        &options.models,
        &options.original_dir,
    ]
    .iter()
    .any(|path| !path.is_absolute())
    {
        return Err("driver controller paths must be absolute".into());
    }
    for path in [&options.root, &options.models] {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("driver checkout or model root is not a real directory".into());
        }
    }
    let original = OriginalCapture::open(
        &options.original_dir,
        &options.original_manifest_sha256,
        &options.source_commit,
    )?;
    if checkout_commit(&options.root)? != options.source_commit {
        return Err("driver source checkout changed from original capture".into());
    }
    let binary = InputPin::open(&options.binary, true)?;
    let peer = InputPin::open(&options.peer, true)?;
    let collector = InputPin::open(&options.collector, true)?;
    let mut model_pins = Vec::new();
    let mut model_hashes = BTreeMap::new();
    for (name, hash) in original.original_models()? {
        let pin = InputPin::open(&options.models.join(name), false)?;
        if pin.digest != hash {
            return Err(format!("driver original model changed: {name}").into());
        }
        model_hashes.insert(name.to_owned(), hash.to_owned());
        model_pins.push(pin);
    }
    let mut source_pins = Vec::new();
    for (name, hash) in original.source_inputs()? {
        real_parent_chain(&options.root, name)?;
        let pin = InputPin::open(&options.root.join(name), false)?;
        if pin.digest != hash {
            return Err(format!("driver source changed: {name}").into());
        }
        source_pins.push(pin);
    }
    let mut imported_pins = Vec::new();
    for (name, hash) in original.loaded_python_sources()? {
        real_parent_chain(&options.root, name)?;
        let pin = InputPin::open(&options.root.join(name), false)?;
        if pin.digest != hash {
            return Err(format!("driver imported Python source changed: {name}").into());
        }
        imported_pins.push(pin);
    }
    let catalog = CatalogPin::open(&options.catalog)?;
    verify_catalog_sources(&catalog, &model_hashes)?;
    fs::create_dir(&options.output)?;
    let mut reports = Vec::with_capacity(CASES.len());
    for (width, height, raw, _) in CASES {
        signals.check()?;
        reports.push(case(&CaseContext {
            options,
            original: &original,
            signals: &signals,
            width,
            height,
            raw,
        })?);
        original.verify()?;
        for pin in [&binary, &peer, &collector] {
            pin.verify()?;
        }
        for pin in &source_pins {
            real_parent_chain(
                &options.root,
                pin.path
                    .strip_prefix(&options.root)?
                    .to_str()
                    .ok_or("non-UTF8 driver source")?,
            )?;
            pin.verify()?;
        }
        for pin in &imported_pins {
            real_parent_chain(
                &options.root,
                pin.path
                    .strip_prefix(&options.root)?
                    .to_str()
                    .ok_or("non-UTF8 driver import")?,
            )?;
            pin.verify()?;
        }
        for pin in &model_pins {
            pin.verify()?;
        }
        catalog.verify()?;
        verify_catalog_sources(&catalog, &model_hashes)?;
    }
    let output = json!({"runs":reports,"device_validation":false,
        "source_commit":options.source_commit,"original_manifest_sha256":options.original_manifest_sha256,
        "inputs":{"binary":binary.receipt(),"peer":peer.receipt(),"collector":collector.receipt(),
            "source_inputs":source_pins.iter().map(InputPin::receipt).collect::<Vec<_>>(),
            "imported_python_sources":imported_pins.iter().map(InputPin::receipt).collect::<Vec<_>>(),
            "original_models":model_pins.iter().map(InputPin::receipt).collect::<Vec<_>>(),
            "catalog":catalog.receipt()}});
    signals.check()?;
    original.verify()?;
    for pin in [&binary, &peer, &collector] {
        pin.verify()?;
    }
    for pin in &source_pins {
        real_parent_chain(
            &options.root,
            pin.path
                .strip_prefix(&options.root)?
                .to_str()
                .ok_or("non-UTF8 driver source")?,
        )?;
        pin.verify()?;
    }
    for pin in &imported_pins {
        real_parent_chain(
            &options.root,
            pin.path
                .strip_prefix(&options.root)?
                .to_str()
                .ok_or("non-UTF8 driver import")?,
        )?;
        pin.verify()?;
    }
    for pin in &model_pins {
        pin.verify()?;
    }
    catalog.verify()?;
    verify_catalog_sources(&catalog, &model_hashes)?;
    if checkout_commit(&options.root)? != options.source_commit {
        return Err("driver checkout changed during live replay".into());
    }
    write_new(
        &options.output.join("report.json"),
        format!("{}\n", serde_json::to_string_pretty(&output)?).as_bytes(),
    )?;
    Ok(output)
}

#[cfg(test)]
mod comparison_tests {
    use super::{
        compare_value, verify_catalog_sources, CatalogPin, Lines, Signals, TransportNamespace,
    };
    use rustix::event::{poll, PollFd, PollFlags};
    use serde_json::json;
    use std::{
        collections::BTreeMap,
        fs::{self, File},
        io::Write,
        os::{
            fd::OwnedFd,
            unix::net::{UnixListener, UnixStream},
        },
        sync::atomic::{AtomicU64, Ordering},
        thread,
        time::Duration,
    };

    static NEXT_NAMESPACE: AtomicU64 = AtomicU64::new(0);
    fn namespace_name() -> String {
        format!(
            "driver-cleanup-test-{}-{}",
            std::process::id(),
            NEXT_NAMESPACE.fetch_add(1, Ordering::Relaxed)
        )
    }

    #[test]
    fn original_numeric_policy_is_narrow_and_counts_fields() {
        let expected = json!({"frameId":11,"modelCoefficients":[100.0,0.0],"scores":[0.5]});
        let accepted = json!({"frameId":11,"modelCoefficients":[100.001,0.0],"scores":[0.5000001]});
        assert_eq!(
            compare_value(&expected, &accepted, "driverStateV2").unwrap(),
            4
        );
        let altered_frame = json!({"frameId":12,"modelCoefficients":[100.0,0.0],"scores":[0.5]});
        assert!(compare_value(&expected, &altered_frame, "driverStateV2").is_err());
        let altered_float = json!({"frameId":11,"modelCoefficients":[100.0,0.0],"scores":[0.501]});
        assert!(compare_value(&expected, &altered_float, "driverStateV2").is_err());
        let missing = json!({"frameId":11,"scores":[0.5]});
        assert!(compare_value(&expected, &missing, "driverStateV2").is_err());
    }

    #[test]
    fn nonfinite_projection_keeps_original_nan_policy() {
        let first = json!({"x":{"float64_bits":"7ff8000000000000"}});
        let other_nan = json!({"x":{"float64_bits":"7ff8000000000001"}});
        let infinity = json!({"x":{"float64_bits":"7ff0000000000000"}});
        assert_eq!(
            compare_value(&first, &other_nan, "driverStateV2").unwrap(),
            1
        );
        assert!(compare_value(&first, &infinity, "driverStateV2").is_err());
    }

    #[test]
    fn both_catalog_driver_pipelines_must_match_original_model_sources() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let generation = "a".repeat(64);
        fs::write(
            root.join("current.json"),
            format!("{{\"version\":1,\"generation\":\"{generation}\"}}"),
        )
        .unwrap();
        fs::create_dir(root.join(&generation)).unwrap();
        let model = "1".repeat(64);
        let metadata = "2".repeat(64);
        let mut models = BTreeMap::from([
            ("dmonitoring_model_tinygrad.pkl".to_owned(), model.clone()),
            (
                "dmonitoring_model_metadata.pkl".to_owned(),
                metadata.clone(),
            ),
        ]);
        let mut bundles = Vec::new();
        for (width, height, warp) in [(1344, 760, "3"), (1928, 1208, "4")] {
            let name = format!("dm_warp_{width}x{height}_tinygrad.pkl");
            models.insert(name, warp.repeat(64));
            let bundle_name = format!("driver-{width}x{height}");
            bundles.push(json!({"kind":"driver","camera":[width,height],"directory":bundle_name}));
            let directory = root.join(&generation).join(bundle_name);
            fs::create_dir(&directory).unwrap();
            fs::write(
                directory.join("pipeline.json"),
                serde_json::to_vec(&json!({
                    "version":1,"kind":"driver","camera":[width,height],
                    "sources":{"model":model,"metadata":metadata,"warp":warp.repeat(64)}
                }))
                .unwrap(),
            )
            .unwrap();
        }
        fs::write(
            root.join(&generation).join("index.json"),
            serde_json::to_vec(&json!({"version":1,"bundles":bundles})).unwrap(),
        )
        .unwrap();
        let catalog = CatalogPin::open(root).unwrap();
        verify_catalog_sources(&catalog, &models).unwrap();
        let alternate = root.join(&generation).join("alternate-driver");
        fs::create_dir(&alternate).unwrap();
        fs::write(
            alternate.join("pipeline.json"),
            serde_json::to_vec(&json!({
                "version":1,"kind":"driver","camera":[1344,760],
                "sources":{"model":"6".repeat(64),"metadata":metadata,"warp":"3".repeat(64)}
            }))
            .unwrap(),
        )
        .unwrap();
        let mut redirected = bundles.clone();
        redirected[0]["directory"] = "alternate-driver".into();
        fs::write(
            root.join(&generation).join("index.json"),
            serde_json::to_vec(&json!({"version":1,"bundles":redirected})).unwrap(),
        )
        .unwrap();
        let redirected_catalog = CatalogPin::open(root).unwrap();
        assert!(
            verify_catalog_sources(&redirected_catalog, &models).is_err(),
            "index-selected wrong-model pipeline bypassed the orphaned canonical descriptor"
        );
        fs::write(
            root.join(&generation).join("index.json"),
            serde_json::to_vec(&json!({"version":1,"bundles":bundles})).unwrap(),
        )
        .unwrap();
        let catalog = CatalogPin::open(root).unwrap();
        models.insert("dm_warp_1928x1208_tinygrad.pkl".into(), "5".repeat(64));
        assert!(
            verify_catalog_sources(&catalog, &models).is_err(),
            "second-camera warp mismatch was accepted"
        );
        models.insert("dm_warp_1928x1208_tinygrad.pkl".into(), "4".repeat(64));
        models.insert("dmonitoring_model_tinygrad.pkl".into(), "6".repeat(64));
        assert!(
            verify_catalog_sources(&catalog, &models).is_err(),
            "model mismatch was accepted"
        );
    }

    #[test]
    fn owned_vision_socket_is_removed_on_success_and_failure() {
        for success in [true, false] {
            let prefix = namespace_name();
            let socket = std::path::Path::new("/tmp").join(format!("{prefix}_visionipc_camerad"));
            let shm = std::path::Path::new("/dev/shm").join(format!("msgq_{prefix}"));
            let mut namespace = TransportNamespace::new(prefix).unwrap();
            let listener = UnixListener::bind(&socket).unwrap();
            assert!(socket.exists());
            if success {
                namespace.finish().unwrap();
            }
            drop(namespace);
            assert!(!socket.exists(), "owned VisionIPC socket leaked");
            assert!(!shm.exists(), "owned shared-memory queue leaked");
            drop(listener);
        }
    }

    #[test]
    fn preexisting_socket_is_not_claimed_or_removed() {
        let prefix = namespace_name();
        let socket = std::path::Path::new("/tmp").join(format!("{prefix}_visionipc_camerad"));
        let listener = UnixListener::bind(&socket).unwrap();
        assert!(TransportNamespace::new(prefix).is_err());
        assert!(socket.exists(), "foreign socket was removed");
        drop(listener);
        fs::remove_file(socket).unwrap();
    }

    #[test]
    fn foreign_file_under_owned_prefix_blocks_success_without_clobber() {
        let prefix = namespace_name();
        let foreign = std::path::Path::new("/tmp").join(format!("{prefix}_visionipc_foreign"));
        let mut namespace = TransportNamespace::new(prefix).unwrap();
        fs::write(&foreign, b"foreign").unwrap();
        assert!(namespace.finish().is_err());
        drop(namespace);
        assert_eq!(fs::read(&foreign).unwrap(), b"foreign");
        fs::remove_file(foreign).unwrap();
    }

    #[test]
    fn interrupted_poll_preserves_line_wait_and_deadline() {
        let temporary = tempfile::tempdir().unwrap();
        let (reader, mut writer) = UnixStream::pair().unwrap();
        reader.set_nonblocking(true).unwrap();
        let fd: OwnedFd = reader.into();
        let mut lines = Lines {
            pipe: File::from(fd),
            log: File::create(temporary.path().join("lines.log")).unwrap(),
            pending: Vec::new(),
            total: 0,
        };
        let signals = Signals::new().unwrap();
        let feeder = thread::spawn(move || {
            thread::sleep(Duration::from_millis(30));
            writer.write_all(b"READY 123\n").unwrap();
        });
        let mut attempts = 0;
        let line = lines
            .until_with_poll(&["READY"], &signals, |pipe, timeout| {
                attempts += 1;
                if attempts == 1 {
                    Err(rustix::io::Errno::INTR)
                } else {
                    poll(&mut [PollFd::new(pipe, PollFlags::IN)], Some(timeout))
                }
            })
            .unwrap();
        feeder.join().unwrap();
        assert_eq!(line, "READY 123");
        assert!(
            attempts >= 2,
            "the injected EINTR did not retry within the same wait"
        );
    }
}
