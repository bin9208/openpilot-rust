//! Native controller for the independently captured original DesireHelper corpus.
//! The candidate never supplies its own expected rows.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use flate2::read::GzDecoder;
use serde::{ser::SerializeMap, Serialize, Serializer};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env,
    ffi::OsString,
    fs::{self, File, Metadata},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const MANIFEST: &[u8] = include_bytes!("../tests/fixtures/desire-original-6e34f78a/manifest.json");
const REQUEST: &[u8] =
    include_bytes!("../tests/fixtures/desire-original-6e34f78a/request.jsonl.gz");
const REFERENCE: &[u8] =
    include_bytes!("../tests/fixtures/desire-original-6e34f78a/reference.jsonl.gz");
const MANIFEST_SHA: &str = "ccb84511e949ccd9ebe44c2dc259cd29f5839a42ec35478a1698311b985a81cf";
const REQUEST_GZ_SHA: &str = "76e4113c7df8cd7b96284c3bfa2b6a22ab112e745e743c70648baa9585097f58";
const REFERENCE_GZ_SHA: &str = "b67d04fe1266cc1e3e240e7bf4195143f0a5e634b31afda4d84e3afe05a07159";
const SOURCE_COMMIT: &str = "6e34f78ae77734dac46967cc605836b950c99db9";
const HISTORICAL_CHECKER: &str = "rust/tools/check_desire_reference.py";
const FRAMES: usize = 24_585;
const TOLERANCE: f64 = 1e-10;
const MAX_ACTUAL: u64 = 128_000_000;
const MAX_SOURCE: u64 = 1_000_000;

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into().into())
    }
}
fn identity(meta: &Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
    (
        meta.dev(),
        meta.ino(),
        meta.len(),
        meta.mtime(),
        meta.mtime_nsec(),
        meta.ctime(),
        meta.ctime_nsec(),
    )
}

struct Bound {
    path: PathBuf,
    file: File,
    meta: Metadata,
    sha: String,
    limit: Option<u64>,
}
struct BoundDir {
    path: PathBuf,
    file: File,
    device: u64,
    inode: u64,
}
impl BoundDir {
    fn open(path: PathBuf) -> Result<Self> {
        let file = File::from(rustix::fs::open(
            &path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        let metadata = file.metadata()?;
        let directory = Self {
            path,
            file,
            device: metadata.dev(),
            inode: metadata.ino(),
        };
        directory.verify()?;
        Ok(directory)
    }
    fn verify(&self) -> Result<()> {
        let current = File::from(rustix::fs::open(
            &self.path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        let now = current.metadata()?;
        let held = self.file.metadata()?;
        require(
            (now.dev(), now.ino()) == (self.device, self.inode)
                && (held.dev(), held.ino()) == (self.device, self.inode),
            format!("desire output directory replaced: {}", self.path.display()),
        )
    }
}
impl Bound {
    fn open(path: PathBuf, expected: Option<&str>, limit: Option<u64>) -> Result<Self> {
        let fd = rustix::fs::open(
            &path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::CLOEXEC
                | rustix::fs::OFlags::NOFOLLOW,
            rustix::fs::Mode::empty(),
        )?;
        let mut file = File::from(fd);
        let meta = file.metadata()?;
        require(
            meta.is_file(),
            format!("nonregular desire input: {}", path.display()),
        )?;
        let sha = Self::hash(&mut file, limit)?;
        require(
            expected.is_none_or(|value| value == sha),
            format!("desire source hash changed: {}", path.display()),
        )?;
        let bound = Self {
            path,
            file,
            meta,
            sha,
            limit,
        };
        bound.verify()?;
        Ok(bound)
    }
    fn hash(file: &mut File, limit: Option<u64>) -> Result<String> {
        file.seek(SeekFrom::Start(0))?;
        let mut hash = Sha256::new();
        let mut total = 0_u64;
        let mut chunk = [0_u8; 65536];
        loop {
            let room = limit.map_or(chunk.len(), |max| {
                (max.saturating_sub(total) + 1).min(chunk.len() as u64) as usize
            });
            let size = file.read(&mut chunk[..room])?;
            if size == 0 {
                break;
            }
            total = total
                .checked_add(size as u64)
                .ok_or("desire file size overflow")?;
            require(
                limit.is_none_or(|max| total <= max),
                "desire output exceeds byte limit",
            )?;
            hash.update(&chunk[..size]);
        }
        Ok(format!("{:x}", hash.finalize()))
    }
    fn verify(&self) -> Result<()> {
        let fd = rustix::fs::open(
            &self.path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::CLOEXEC
                | rustix::fs::OFlags::NOFOLLOW,
            rustix::fs::Mode::empty(),
        )?;
        let mut current = File::from(fd);
        let meta = current.metadata()?;
        require(
            meta.is_file()
                && identity(&meta) == identity(&self.meta)
                && identity(&self.file.metadata()?) == identity(&self.meta)
                && Self::hash(&mut current, self.limit)? == self.sha,
            format!("desire input changed: {}", self.path.display()),
        )
    }
    fn native_elf(&mut self) -> Result<()> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut header = [0_u8; 64];
        self.file.read_exact(&mut header)?;
        let machine = match env::consts::ARCH {
            "x86_64" => 62,
            "aarch64" => 183,
            _ => return Err("unsupported native desire host".into()),
        };
        require(
            &header[..4] == b"\x7fELF"
                && header[4] == 2
                && header[5] == 1
                && header[6] == 1
                && matches!(u16::from_le_bytes([header[16], header[17]]), 2 | 3)
                && u16::from_le_bytes([header[18], header[19]]) == machine,
            "desire candidate is not a native host ELF",
        )?;
        self.verify()
    }
}

fn absent(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error.into()),
        Ok(_) => Ok(false),
    }
}
fn verify_all(
    directory: &BoundDir,
    bounds: &[Bound],
    missing: &[PathBuf],
    stop: &AtomicBool,
) -> Result<()> {
    require(
        !stop.load(Ordering::SeqCst),
        "desire reference comparison interrupted",
    )?;
    directory.verify()?;
    for bound in bounds {
        bound.verify()?;
    }
    for path in missing {
        require(
            absent(path)?,
            format!("historical desire checker reappeared: {}", path.display()),
        )?;
    }
    Ok(())
}
fn bindings(bounds: &[Bound], missing: &[PathBuf]) -> Value {
    serde_json::json!({
        "present": bounds.iter().map(|bound| serde_json::json!({
            "path_bytes":bound.path.as_os_str().as_bytes(), "sha256":bound.sha,
            "device":bound.meta.dev(), "inode":bound.meta.ino(), "bytes":bound.meta.len()
        })).collect::<Vec<_>>(),
        "historical_absent": missing.iter().map(|path| path.as_os_str().as_bytes()).collect::<Vec<_>>()
    })
}

#[derive(Debug)]
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

fn manifest() -> Result<Value> {
    require(
        digest(MANIFEST) == MANIFEST_SHA,
        "desire original manifest changed",
    )?;
    require(
        digest(REQUEST) == REQUEST_GZ_SHA && digest(REFERENCE) == REFERENCE_GZ_SHA,
        "desire original compressed streams changed",
    )?;
    let value: Value = serde_json::from_slice(MANIFEST)?;
    require(
        value["source_commit"] == SOURCE_COMMIT
            && value["frames"] == FRAMES
            && value["scenario_count"] == 47,
        "desire original manifest identity changed",
    )?;
    Ok(value)
}

fn restore(compressed: &[u8], expected: &Value, path: &Path) -> Result<()> {
    let expected_size = expected["bytes"]
        .as_u64()
        .ok_or("desire original byte count missing")?;
    let expected_hash = expected["sha256"]
        .as_str()
        .ok_or("desire original SHA missing")?;
    let mut decoder = GzDecoder::new(compressed);
    let mut target = File::create_new(path)?;
    let mut hash = Sha256::new();
    let mut total = 0_u64;
    let mut chunk = [0_u8; 65536];
    loop {
        let count = decoder.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .ok_or("desire original size overflow")?;
        require(
            total <= expected_size,
            "desire original stream exceeds sealed size",
        )?;
        hash.update(&chunk[..count]);
        target.write_all(&chunk[..count])?;
    }
    target.sync_all()?;
    require(
        total == expected_size && format!("{:x}", hash.finalize()) == expected_hash,
        "desire original stream hash or size differs",
    )
}

fn python_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Bool(a), Value::Number(b)) => b.as_f64() == Some(f64::from(u8::from(*a))),
        (Value::Number(a), Value::Bool(b)) => a.as_f64() == Some(f64::from(u8::from(*b))),
        (Value::Number(a), Value::Number(b)) => a == b || a.as_f64() == b.as_f64(),
        _ => left == right,
    }
}

pub fn compare_values(expected: &Value, actual: &Value, path: &str) -> Result<f64> {
    match expected {
        Value::Object(left) => {
            let right = actual
                .as_object()
                .ok_or_else(|| format!("{path}: expected object"))?;
            require(
                left.len() == right.len() && left.keys().all(|key| right.contains_key(key)),
                format!("{path}: object keys differ"),
            )?;
            let mut max: f64 = 0.0;
            for (key, value) in left {
                max = max.max(compare_values(
                    value,
                    &right[key],
                    &format!("{path}.{key}"),
                )?);
            }
            Ok(max)
        }
        Value::Array(left) => {
            let right = actual
                .as_array()
                .ok_or_else(|| format!("{path}: expected array"))?;
            require(
                left.len() == right.len(),
                format!("{path}: array length differs"),
            )?;
            let mut max: f64 = 0.0;
            for (index, (a, b)) in left.iter().zip(right).enumerate() {
                max = max.max(compare_values(a, b, &format!("{path}[{index}]"))?);
            }
            Ok(max)
        }
        Value::Number(number) if number.is_f64() => {
            let a = number.as_f64().ok_or("invalid original desire float")?;
            let b = match actual {
                Value::Bool(flag) => f64::from(u8::from(*flag)),
                Value::Number(value) => value.as_f64().ok_or("invalid candidate desire number")?,
                _ => return Err(format!("{path}: candidate is not numeric").into()),
            };
            let error = (a - b).abs();
            require(
                error <= TOLERANCE,
                format!("{path}: numerical difference {a:?} != {b:?}"),
            )?;
            Ok(error)
        }
        _ => {
            require(
                python_equal(expected, actual),
                format!("{path}: discrete value differs"),
            )?;
            Ok(0.0)
        }
    }
}

#[derive(Debug)]
struct Coverage(Vec<(String, usize)>);
impl Serialize for Coverage {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, count) in &self.0 {
            map.serialize_entry(key, count)?;
        }
        map.end()
    }
}
#[derive(Debug, Serialize)]
pub struct Report {
    frames: usize,
    maximum_absolute_error: f64,
    numeric_tolerance: f64,
    state_coverage: Coverage,
    device_validation: bool,
}
pub fn compare_files(
    request: &Path,
    reference: &Path,
    actual: &Path,
    sealed: &Value,
    expected_frames: usize,
) -> Result<Report> {
    let mut requests = BufReader::new(File::open(request)?).lines();
    let mut originals = BufReader::new(File::open(reference)?).lines();
    let mut observed = BufReader::new(File::open(actual)?).lines();
    let mut scenarios = BTreeMap::<String, usize>::new();
    let mut coverage = Coverage(Vec::new());
    let mut maximum_absolute_error: f64 = 0.0;
    let mut frames = 0;
    loop {
        let (request, original, candidate) =
            match (requests.next(), originals.next(), observed.next()) {
                (None, None, None) => break,
                (Some(request), Some(original), Some(candidate)) => (request, original, candidate),
                _ => return Err("desire streams have different line counts".into()),
            };
        let request: Value = serde_json::from_str(&request?)?;
        let original: Value = serde_json::from_str(&original?)?;
        let candidate: Value = serde_json::from_str(&candidate?)?;
        require(
            keys(
                &request,
                &["reset", "input", "config", "remote", "scenario"],
            ) && keys(&original, &["state", "remote_allowed", "params_refreshed"]),
            format!("desire original schema changed at {frames}"),
        )?;
        let scenario = request["scenario"]
            .as_str()
            .ok_or("desire scenario not text")?;
        let prior = *scenarios.get(scenario).unwrap_or(&0);
        require(
            request["reset"].as_bool() == Some(prior == 0),
            format!("desire reset boundary changed at {frames}"),
        )?;
        *scenarios.entry(scenario.to_owned()).or_default() += 1;
        maximum_absolute_error =
            maximum_absolute_error.max(compare_values(&original, &candidate, &frames.to_string())?);
        let state = original["state"]["lane_change_state"]
            .as_str()
            .ok_or("desire state missing")?;
        let desire = original["state"]["desire"]
            .as_i64()
            .ok_or("desire code missing")?;
        for label in [format!("state:{state}"), format!("desire:{desire}")] {
            if let Some((_, count)) = coverage.0.iter_mut().find(|(key, _)| key == &label) {
                *count += 1;
            } else {
                coverage.0.push((label, 1));
            }
        }
        frames += 1;
    }
    require(
        frames == expected_frames
            && sealed["scenarios"] == serde_json::to_value(&scenarios)?
            && sealed["state_coverage"]
                == serde_json::to_value(BTreeMap::from_iter(coverage.0.iter().cloned()))?,
        "desire original scenario or state coverage changed",
    )?;
    Ok(Report {
        frames,
        maximum_absolute_error,
        numeric_tolerance: TOLERANCE,
        state_coverage: coverage,
        device_validation: false,
    })
}
fn keys(value: &Value, required: &[&str]) -> bool {
    value.as_object().is_some_and(|map| {
        map.len() == required.len() && required.iter().all(|key| map.contains_key(*key))
    })
}

pub struct Completed {
    report: Report,
    output: PathBuf,
    directory: BoundDir,
    bounds: Vec<Bound>,
    missing: Vec<PathBuf>,
}
pub fn prepare(options: &Options, runner: &mut dyn Runner, stop: &AtomicBool) -> Result<Completed> {
    let sealed = manifest()?;
    if let Some(parent) = options
        .output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&options.output)?;
    let output = fs::canonicalize(&options.output)?;
    let directory = BoundDir::open(output.clone())?;
    let root = fs::canonicalize(&options.root)?;
    let source = sealed["source"]
        .as_object()
        .ok_or("desire source manifest missing")?;
    let mut bounds = Vec::new();
    let mut missing = Vec::new();
    for (name, row) in source {
        let expected = row["before_sha256"]
            .as_str()
            .ok_or("desire source SHA missing")?;
        require(
            row["after_sha256"] == expected,
            format!("desire original source mutated: {name}"),
        )?;
        let path = root.join(name);
        if name == HISTORICAL_CHECKER && absent(&path)? {
            missing.push(path);
        } else {
            bounds.push(Bound::open(path, Some(expected), Some(MAX_SOURCE))?);
        }
    }
    let binary = fs::canonicalize(&options.binary)?;
    let mut candidate = Bound::open(binary.clone(), None, None)?;
    candidate.native_elf()?;
    bounds.push(candidate);
    verify_all(&directory, &bounds, &missing, stop)?;
    let request = output.join("request.jsonl");
    let reference = output.join("reference.jsonl");
    restore(REQUEST, &sealed["files"]["request.jsonl"], &request)?;
    restore(REFERENCE, &sealed["files"]["reference.jsonl"], &reference)?;
    bounds.push(Bound::open(
        request.clone(),
        sealed["files"]["request.jsonl"]["sha256"].as_str(),
        Some(
            sealed["files"]["request.jsonl"]["bytes"]
                .as_u64()
                .ok_or("sealed desire request byte count missing")?,
        ),
    )?);
    bounds.push(Bound::open(
        reference.clone(),
        sealed["files"]["reference.jsonl"]["sha256"].as_str(),
        Some(
            sealed["files"]["reference.jsonl"]["bytes"]
                .as_u64()
                .ok_or("sealed desire reference byte count missing")?,
        ),
    )?);
    fs::write(output.join("original-only-manifest.json"), MANIFEST)?;
    let actual = output.join("actual.jsonl");
    let mut command = CommandSpec::new_os(
        vec![
            binary.into_os_string(),
            request.into_os_string(),
            actual.as_os_str().to_owned(),
        ],
        Duration::from_secs(300),
    );
    command.stdout_limit = 16 * 1024 * 1024;
    command.stderr_limit = 16 * 1024 * 1024;
    verify_all(&directory, &bounds, &missing, stop)?;
    fs::write(
        output.join("bindings-before.json"),
        serde_json::to_vec_pretty(&bindings(&bounds, &missing))?,
    )?;
    let captured = runner.run_to_files(
        &command,
        &output.join("stdout.log"),
        &output.join("stderr.log"),
    );
    let Captured { code, .. } = match captured {
        Ok(value) => value,
        Err(error) => {
            fs::write(
                output.join("process.json"),
                serde_json::to_vec_pretty(
                    &serde_json::json!({"error":error.to_string(),"timeout_seconds":300}),
                )?,
            )?;
            return Err(error);
        }
    };
    fs::write(
        output.join("process.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"exit_code":code,"timeout_seconds":300}))?,
    )?;
    verify_all(&directory, &bounds, &missing, stop)?;
    require(code == 0, format!("desire candidate exited {code}"))?;
    bounds.push(Bound::open(
        output.join("stdout.log"),
        None,
        Some(16 * 1024 * 1024),
    )?);
    bounds.push(Bound::open(
        output.join("stderr.log"),
        None,
        Some(16 * 1024 * 1024),
    )?);
    let actual_bound = Bound::open(actual.clone(), None, Some(MAX_ACTUAL))?;
    let report = compare_files(
        &output.join("request.jsonl"),
        &output.join("reference.jsonl"),
        &actual,
        &sealed,
        FRAMES,
    )?;
    bounds.push(actual_bound);
    verify_all(&directory, &bounds, &missing, stop)?;
    fs::write(
        output.join("bindings-after.json"),
        serde_json::to_vec_pretty(&bindings(&bounds, &missing))?,
    )?;
    Ok(Completed {
        report,
        output,
        directory,
        bounds,
        missing,
    })
}

struct Receipt {
    path: PathBuf,
    file: File,
    committed: bool,
}
impl Receipt {
    fn owns(&self) -> bool {
        self.file.metadata().is_ok_and(|held| {
            fs::symlink_metadata(&self.path).is_ok_and(|current| {
                current.is_file() && (held.dev(), held.ino()) == (current.dev(), current.ino())
            })
        })
    }
}
impl Drop for Receipt {
    fn drop(&mut self) {
        if !self.committed && self.owns() {
            let _ = fs::remove_file(&self.path);
        }
    }
}
fn finish_after_teardown(
    done: Completed,
    stdout: &mut dyn Write,
    stop: &AtomicBool,
    teardown: impl FnOnce(),
) -> Result<Report> {
    verify_all(&done.directory, &done.bounds, &done.missing, stop)?;
    std::io::copy(&mut File::open(done.output.join("stdout.log"))?, stdout)?;
    writeln!(stdout, "{}", serde_json::to_string(&done.report)?)?;
    stdout.flush()?;
    verify_all(&done.directory, &done.bounds, &done.missing, stop)?;
    let path = done.output.join("report.json");
    let mut pending = tempfile::NamedTempFile::new_in(&done.output)?;
    writeln!(pending, "{}", serde_json::to_string_pretty(&done.report)?)?;
    verify_all(&done.directory, &done.bounds, &done.missing, stop)?;
    let file = pending
        .persist_noclobber(&path)
        .map_err(|error| error.error)?;
    let mut receipt = Receipt {
        path,
        file,
        committed: false,
    };
    teardown();
    verify_all(&done.directory, &done.bounds, &done.missing, stop)?;
    require(
        receipt.owns(),
        "desire report receipt replaced before commit",
    )?;
    receipt.committed = true;
    Ok(done.report)
}
pub fn finish(done: Completed, stdout: &mut dyn Write, stop: &AtomicBool) -> Result<Report> {
    finish_after_teardown(done, stdout, stop, || {})
}
struct SignalRunner(Arc<AtomicBool>);
impl Runner for SignalRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<Captured> {
        NativeRunner.run_controlled(command, Instant::now() + command.timeout, &self.0)
    }
}
struct Signals(Vec<signal_hook::SigId>);
impl Drop for Signals {
    fn drop(&mut self) {
        for id in self.0.drain(..) {
            signal_hook::low_level::unregister(id);
        }
    }
}
pub fn cli() -> Result<()> {
    let Some(options) = parse(&env::args_os().skip(1).collect::<Vec<_>>())? else {
        println!("usage: check-desire-reference-native --binary BINARY --output DIRECTORY");
        return Ok(());
    };
    let stop = Arc::new(AtomicBool::new(false));
    let mut signals = Signals(Vec::new());
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signals
            .0
            .push(signal_hook::flag::register(signal, stop.clone())?);
    }
    let prepared = prepare(&options, &mut SignalRunner(stop.clone()), &stop)?;
    std::io::copy(
        &mut File::open(prepared.output.join("stderr.log"))?,
        &mut std::io::stderr(),
    )?;
    finish_after_teardown(prepared, &mut std::io::stdout(), &stop, || drop(signals))?;
    Ok(())
}

#[cfg(test)]
mod finalization_tests {
    use super::*;
    use std::io;

    fn completed(root: &Path) -> (Completed, PathBuf, PathBuf) {
        let source = root.join("source");
        fs::write(&source, b"source").unwrap();
        let output = root.join("output");
        fs::create_dir(&output).unwrap();
        fs::write(output.join("stdout.log"), b"").unwrap();
        let staged = root.join("staged-output");
        fs::create_dir(&staged).unwrap();
        fs::hard_link(output.join("stdout.log"), staged.join("stdout.log")).unwrap();
        let directory = BoundDir::open(output.clone()).unwrap();
        let absent = root.join("historical.py");
        (
            Completed {
                report: Report {
                    frames: 2,
                    maximum_absolute_error: 0.0,
                    numeric_tolerance: TOLERANCE,
                    state_coverage: Coverage(vec![("state:Off".into(), 2)]),
                    device_validation: false,
                },
                output,
                directory,
                bounds: vec![
                    Bound::open(source.clone(), None, None).unwrap(),
                    Bound::open(root.join("output/stdout.log"), None, None).unwrap(),
                ],
                missing: vec![absent.clone()],
            },
            source,
            absent,
        )
    }
    struct MutatingWriter<'a> {
        source: &'a Path,
        mutate: bool,
    }
    impl Write for MutatingWriter<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            if self.mutate {
                fs::write(self.source, b"late mutation")?;
            }
            Ok(())
        }
    }
    #[test]
    fn final_stdout_and_teardown_recheck_sources_absence_and_owned_receipt() {
        for phase in ["control", "stdout", "reappear", "foreign", "replacement"] {
            let root = tempfile::tempdir().unwrap();
            let (done, source, absent) = completed(root.path());
            let receipt = done.output.join("report.json");
            let unchanged = (phase == "replacement")
                .then(|| Bound::open(root.path().join("output/stdout.log"), None, None).unwrap());
            let stop = AtomicBool::new(false);
            let result = finish_after_teardown(
                done,
                &mut MutatingWriter {
                    source: &source,
                    mutate: phase == "stdout",
                },
                &stop,
                || match phase {
                    "reappear" => fs::write(&absent, b"reappeared").unwrap(),
                    "foreign" => {
                        fs::remove_file(&receipt).unwrap();
                        fs::write(&receipt, b"foreign receipt").unwrap();
                    }
                    "replacement" => {
                        let moved = root.path().join("moved-output");
                        fs::rename(root.path().join("output"), &moved).unwrap();
                        fs::rename(
                            root.path().join("staged-output"),
                            root.path().join("output"),
                        )
                        .unwrap();
                        fs::write(&receipt, b"foreign receipt").unwrap();
                        unchanged.as_ref().unwrap().verify().unwrap();
                    }
                    _ => {}
                },
            );
            assert_eq!(result.is_ok(), phase == "control", "{phase}");
            match phase {
                "control" => assert!(receipt.is_file()),
                "foreign" | "replacement" => {
                    assert_eq!(fs::read(&receipt).unwrap(), b"foreign receipt")
                }
                _ => assert!(!receipt.exists(), "{phase} left owned success receipt"),
            }
        }
    }
}
