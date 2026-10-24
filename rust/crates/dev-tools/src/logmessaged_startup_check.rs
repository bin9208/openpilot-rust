//! Independent native controller for the original three startup observations.
//! The shared Python adapters remain retained inputs; no Python is executed.
use crate::{
    argparse_scan::{scan_os, ArgumentError, OptionSpec},
    path_resolve::resolve_non_strict,
    qa_process::{CommandSpec, NativeRunner},
    seal::Result,
    stats_daemon_check::identity::Bound,
};
use openpilot_log_format::JsonValue;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File},
    io::Write,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
mod worker;

pub const CASES: [&str; 3] = ["legacy-source", "ready-source", "ready-native"];
pub const MARKER: &str = r#"{"msg": "first-error-after-start", "levelnum": 40}"#;
const HISTORICAL_CHECKER: &str = "rust/tools/check_logmessaged_startup.py";
const OBSERVATION_SHA256: &str = "6b3a28eb057aac4dc3bdf3088015711e7bf684acda5e8c40cb2af2d52b92ef15";
pub struct Options {
    pub repository: PathBuf,
    pub collector: PathBuf,
    pub output: PathBuf,
}
fn require(value: bool, reason: &str) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(reason.to_owned().into())
    }
}
/// Original observed strings and file predicates, independent of the candidate.
pub fn validate_observed(case: &str, observed: &Value, disk: &[Value]) -> Result<Value> {
    require(CASES.contains(&case), "unknown startup case")?;
    let error = if case == "legacy-source" {
        json!([])
    } else {
        json!([MARKER])
    };
    require(
        observed == &json!({"logMessage":[MARKER],"errorLogMessage":error}),
        "startup publication differs from original first-error observation",
    )?;
    require(
        disk.len() == 1 && disk[0]["msg$s"] == "first-error-after-start",
        "startup must write exactly one first-error record",
    )?;
    Ok(json!({"logMessage":1,"errorLogMessage":if case=="legacy-source" {0} else {1}}))
}
struct Inputs {
    held: Vec<Bound>,
    hashes: BTreeMap<String, String>,
    historical_checker: Option<Bound>,
    historical_path: PathBuf,
    historical_hash: String,
}
impl Inputs {
    fn new(repository: &Path, executable: &Path) -> Result<Self> {
        require(
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../tests/fixtures/logmessaged-startup/original-observation.json"
                ))
            ) == OBSERVATION_SHA256,
            "startup original observation digest differs",
        )?;
        let pins: BTreeMap<String, String> = serde_json::from_str(include_str!(
            "../tests/fixtures/logmessaged-startup/source-pins.json"
        ))?;
        let historical_hash = pins
            .get(HISTORICAL_CHECKER)
            .ok_or("historical startup checker pin missing")?
            .clone();
        let mut result = Self {
            held: vec![],
            hashes: BTreeMap::new(),
            historical_checker: None,
            historical_path: repository.join(HISTORICAL_CHECKER),
            historical_hash,
        };
        for (name, expected) in pins {
            if name == HISTORICAL_CHECKER {
                match fs::symlink_metadata(&result.historical_path) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(error.into()),
                    Ok(metadata) if metadata.file_type().is_file() => (),
                    Ok(_) => return Err("historical startup checker is not a regular file".into()),
                }
                let held = Bound::open(&result.historical_path)?;
                require(held.hash == expected, "historical startup checker changed")?;
                result.historical_checker = Some(held);
            } else {
                result.bind(repository.join(&name), &name, Some(&expected))?;
            }
        }
        // These are the authored mechanical boundary files compiled into this
        // executable, not a claim about every transitive dependency's build.
        for (name, bytes) in [
            (
                "rust/crates/dev-tools/src/logmessaged_startup_check.rs",
                &include_bytes!("logmessaged_startup_check.rs")[..],
            ),
            (
                "rust/crates/dev-tools/src/logmessaged_startup_check/worker.rs",
                &include_bytes!("logmessaged_startup_check/worker.rs")[..],
            ),
            (
                "rust/crates/dev-tools/src/bin/check_logmessaged_startup.rs",
                &include_bytes!("bin/check_logmessaged_startup.rs")[..],
            ),
            (
                "rust/crates/dev-tools/src/managed_entry_collector_source.rs",
                &include_bytes!("managed_entry_collector_source.rs")[..],
            ),
            (
                "rust/crates/dev-tools/src/stats_daemon_check/process.rs",
                &include_bytes!("stats_daemon_check/process.rs")[..],
            ),
            (
                "rust/crates/dev-tools/src/stats_daemon_check/identity.rs",
                &include_bytes!("stats_daemon_check/identity.rs")[..],
            ),
            (
                "rust/crates/dev-tools/src/qa_endpoint.rs",
                &include_bytes!("qa_endpoint.rs")[..],
            ),
            (
                "rust/crates/dev-tools/src/qa_process.rs",
                &include_bytes!("qa_process.rs")[..],
            ),
            (
                "rust/crates/dev-tools/src/argparse_scan.rs",
                &include_bytes!("argparse_scan.rs")[..],
            ),
            (
                "rust/crates/dev-tools/src/python_fs.rs",
                &include_bytes!("python_fs.rs")[..],
            ),
            (
                "rust/crates/dev-tools/src/path_resolve.rs",
                &include_bytes!("path_resolve.rs")[..],
            ),
            (
                "rust/crates/dev-tools/tests/fixtures/logmessaged-startup/source-pins.json",
                &include_bytes!("../tests/fixtures/logmessaged-startup/source-pins.json")[..],
            ),
            (
                "rust/crates/dev-tools/tests/fixtures/logmessaged-startup/original-observation.json",
                &include_bytes!("../tests/fixtures/logmessaged-startup/original-observation.json")[..],
            ),
        ] {
            result.bind(
                repository.join(name),
                name,
                Some(&format!("{:x}", Sha256::digest(bytes))),
            )?;
        }
        result.bind(executable.to_owned(), "checker", None)?;
        Ok(result)
    }
    fn bind(&mut self, path: PathBuf, name: &str, expected: Option<&str>) -> Result<()> {
        let held = Bound::open(&path)?;
        require(
            expected.is_none_or(|expected| expected == held.hash),
            "startup input hash differs",
        )?;
        self.hashes.insert(name.into(), held.hash.clone());
        self.held.push(held);
        Ok(())
    }
    fn verify(&self) -> Result<()> {
        for input in &self.held {
            input.verify()?;
        }
        if let Some(checker) = &self.historical_checker {
            require(
                fs::symlink_metadata(&self.historical_path)?
                    .file_type()
                    .is_file(),
                "historical startup checker path changed type",
            )?;
            checker.verify()?;
        } else {
            match fs::symlink_metadata(&self.historical_path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
                Ok(_) => return Err("historical startup checker reappeared".into()),
            }
        }
        Ok(())
    }
}
struct Namespace {
    shm: tempfile::TempDir,
    _socket: tempfile::TempDir,
    endpoint: crate::qa_endpoint::EndpointOwner,
    prefix: String,
}
impl Namespace {
    fn new() -> Result<Self> {
        let shm = tempfile::Builder::new()
            .prefix("msgq_log-startup-")
            .tempdir_in("/dev/shm")?;
        let prefix = shm
            .path()
            .file_name()
            .and_then(|v| v.to_str())
            .and_then(|v| v.strip_prefix("msgq_"))
            .ok_or("namespace encoding")?
            .to_owned();
        let endpoint = crate::qa_endpoint::EndpointOwner::new(
            &PathBuf::from(format!("/tmp/logmessage{prefix}")),
            &prefix,
        )?;
        let socket = tempfile::Builder::new()
            .prefix("log-startup-")
            .tempdir_in("/tmp")?;
        Ok(Self {
            shm,
            _socket: socket,
            endpoint,
            prefix,
        })
    }
}
fn run_case(
    options: &Options,
    case: &str,
    executable: &Path,
    cancel: &AtomicBool,
    inputs: &mut Inputs,
) -> Result<Value> {
    let directory = options.output.join(case);
    fs::create_dir(&directory)?;
    let gate_path = directory.join("publisher.gate");
    let _gate = Gate(FileOwner {
        file: File::create_new(&gate_path)?,
        path: gate_path,
    });
    let namespace = Namespace::new()?;
    let mut command = CommandSpec::new_os(
        vec![
            executable.as_os_str().to_owned(),
            "--startup-worker".into(),
            case.into(),
            options.collector.as_os_str().to_owned(),
            directory.as_os_str().to_owned(),
            namespace._socket.path().as_os_str().to_owned(),
        ],
        Duration::from_secs(65),
    );
    command.set_os_env(BTreeMap::from([
        (
            OsString::from("OPENPILOT_PREFIX"),
            OsString::from(&namespace.prefix),
        ),
        (
            OsString::from("HOME"),
            directory.join("collector/home").into_os_string(),
        ),
        (
            OsString::from("STARTUP_ENDPOINT_OWNER"),
            namespace.endpoint.channel().as_os_str().to_owned(),
        ),
    ]));
    inputs.verify()?;
    let output = NativeRunner.run_controlled(&command, Instant::now() + command.timeout, cancel)?;
    fs::write(directory.join("worker.stdout"), &output.stdout)?;
    fs::write(directory.join("worker.stderr"), &output.stderr)?;
    require(
        output.code == 0,
        &format!(
            "{case} worker exited {}: {}",
            output.code,
            String::from_utf8_lossy(&output.stderr)
        ),
    )?;
    let result: Value = serde_json::from_slice(&output.stdout)?;
    namespace.endpoint.verify()?;
    if case == "ready-native" {
        inputs.bind(
            options.collector.clone(),
            "collector",
            Some(
                result["collector_sha256"]
                    .as_str()
                    .ok_or("native collector hash absent")?,
            ),
        )?;
    }
    inputs.verify()?;
    require(
        !cancel.load(Ordering::Relaxed),
        "startup comparison cancelled",
    )?;
    drop(namespace.endpoint);
    namespace.shm.close()?;
    let counts = &result["counts"];
    require(
        counts == &json!({"logMessage":1,"errorLogMessage":if case=="legacy-source" {0} else {1}}),
        "worker observation summary differs",
    )?;
    Ok(counts.clone())
}
struct FileOwner {
    file: File,
    path: PathBuf,
}
impl FileOwner {
    fn remove(&self) {
        if let (Ok(owned), Ok(now)) = (self.file.metadata(), fs::symlink_metadata(&self.path)) {
            if (owned.dev(), owned.ino()) == (now.dev(), now.ino()) {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}
struct Gate(FileOwner);
impl Drop for Gate {
    fn drop(&mut self) {
        self.0.remove();
    }
}
fn publish(path: &Path, bytes: &[u8], inputs: &Inputs, cancel: &AtomicBool) -> Result<FileOwner> {
    inputs.verify()?;
    require(
        !cancel.load(Ordering::Relaxed),
        "startup comparison cancelled",
    )?;
    let mut pending = tempfile::NamedTempFile::new_in(path.parent().ok_or("receipt parent")?)?;
    pending.write_all(bytes)?;
    let held = FileOwner {
        file: pending
            .persist_noclobber(path)
            .map_err(|error| error.error)?,
        path: path.to_owned(),
    };
    let result = inputs.verify().and_then(|()| {
        require(
            !cancel.load(Ordering::Relaxed),
            "startup comparison cancelled",
        )
    });
    if let Err(error) = result {
        held.remove();
        return Err(error);
    }
    Ok(held)
}
pub fn run(options: &Options, executable: &Path) -> Result<JsonValue> {
    execute(options, executable, &AtomicBool::new(false), false).map(|(report, _receipt)| report)
}
fn execute(
    options: &Options,
    executable: &Path,
    cancel: &AtomicBool,
    announce: bool,
) -> Result<(JsonValue, FileOwner)> {
    // Original mkdir is exclusive: never overwrite or remove a foreign receipt.
    if let Some(parent) = options.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&options.output)?;
    let mut inputs = Inputs::new(&options.repository, executable)?;
    fs::write(options.output.join("compiler.stdout"), [])?;
    fs::write(options.output.join("compiler.stderr"), [])?;
    let mut cases = Vec::new();
    for case in CASES {
        let counts = run_case(options, case, executable, cancel, &mut inputs)?;
        cases.push(format!(
            "{}:{{\"logMessage\":{},\"errorLogMessage\":{}}}",
            serde_json::to_string(case)?,
            counts["logMessage"],
            counts["errorLogMessage"]
        ));
    }
    // No compiler is executed by the native controller. Keep the original
    // fixture hash, and describe the actual native gate rather than inventing
    // a successful historical C compilation in the compiler field.
    let report=JsonValue::parse(&format!(
        "{{\"passed\":true,\"cases\":{{{}}},\"compiler\":[],\"fixture_sha256\":{},\"initialization_gate\":\"native source publisher reset handshake\",\"input_sha256\":{},\"historical_source_sha256\":{{\"{}\":{}}},\"reference_observation_sha256\":{}}}",
        cases.join(","),serde_json::to_string(inputs.hashes.get("rust/tools/fixtures/logmessaged_publisher_pause.c").ok_or("pause fixture hash missing")?)?,serde_json::to_string(&inputs.hashes)?,HISTORICAL_CHECKER,serde_json::to_string(&inputs.historical_hash)?,serde_json::to_string(OBSERVATION_SHA256)?))?;
    let bytes = report.to_json()?;
    if announce {
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(bytes.as_bytes())?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
    }
    let receipt = publish(
        &options.output.join("report.json"),
        bytes.as_bytes(),
        &inputs,
        cancel,
    )?;
    Ok((report, receipt))
}
pub fn parse(args: &[OsString]) -> Result<Option<Options>> {
    let parsed = scan_os(
        args,
        &[
            OptionSpec::value("--output", true),
            OptionSpec::value("--collector", true),
        ],
    )?;
    if parsed.help {
        return Ok(None);
    }
    let repository = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    Ok(Some(Options {
        repository,
        collector: resolve_non_strict(Path::new(
            parsed.values.get("--collector").ok_or("collector")?,
        ))?,
        output: resolve_non_strict(Path::new(parsed.values.get("--output").ok_or("output")?))?,
    }))
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
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|v| v == "--startup-source") {
        return worker::source(&args[1..]);
    }
    if args.first().is_some_and(|v| v == "--startup-worker") {
        return worker::run(&args[1..]);
    }
    let Some(options) = parse(&args)? else {
        println!("check-logmessaged-startup --collector PATH --output NEW_DIRECTORY");
        return Ok(());
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = Arc::new(AtomicUsize::new(0));
    let mut signals = Signals(vec![]);
    for signum in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signals
            .0
            .push(signal_hook::flag::register(signum, cancelled.clone())?);
        signals.0.push(signal_hook::flag::register_usize(
            signum,
            signal.clone(),
            signum as usize,
        )?);
    }
    let result = execute(&options, &std::env::current_exe()?, &cancelled, true);
    if let Ok(signum) = i32::try_from(signal.load(Ordering::Relaxed)) {
        if signum != 0 {
            if let Ok((_, receipt)) = &result {
                receipt.remove();
            }
            signal_hook::low_level::emulate_default_handler(signum)?;
            return Err("startup comparison interrupted".into());
        }
    }
    result.map(|_| ())
}
pub fn argument_error(error: &(dyn std::error::Error + 'static)) -> bool {
    error.is::<ArgumentError>()
}
