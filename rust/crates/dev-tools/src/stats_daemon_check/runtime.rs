//! Real production entrypoint and busy-drain shutdown checks derived from the
//! unchanged Python controllers. No captured candidate answers are loaded here.
use super::{identity::Bound, process::check, Namespace, Process, Signals};
use crate::{path_resolve::resolve_non_strict, seal::Result};
use openpilot_log_format::JsonValue;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, BufRead, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{atomic::AtomicBool, Arc},
};
mod live;

pub struct Options {
    pub root: PathBuf,
    pub binaries: PathBuf,
    pub output: PathBuf,
}
fn require(value: bool, message: &str) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(message.to_owned().into())
    }
}

pub fn validate_runtime(report: &Value, count: usize, mode: u32) -> Result<()> {
    require(
        count == 1 && mode & 0o777 == 0o600,
        "runtime file count/mode differs",
    )?;
    let text = report["file"].as_str().ok_or("runtime file is not text")?;
    require(
        text.starts_with("gauge.runtime,started=")
            && text.contains(" value=7.0,dongle_id=\"None\" "),
        "runtime metric differs",
    )?;
    let stamp: u64 = text
        .rsplit(' ')
        .next()
        .ok_or("missing timestamp")?
        .trim()
        .parse()?;
    let before = report["before_ns"].as_u64().ok_or("missing before_ns")?;
    let after = report["after_ns"].as_u64().ok_or("missing after_ns")?;
    require(
        before <= stamp && stamp <= after,
        "runtime timestamp outside real clock bounds",
    )?;
    require(
        report["file_timestamp_ns"].as_u64() == Some(stamp),
        "runtime timestamp receipt differs",
    )?;
    require(report["exit"] == 0, "runtime shutdown did not exit0")
}
pub fn validate_flood(report: &Value, files: usize) -> Result<()> {
    require(
        report["exit"] == 0 && report["producers_alive"] == true,
        "busy shutdown did not exit0 with both producers alive",
    )?;
    require(
        report["busy_cpu_ticks"].as_u64().is_some_and(|n| n >= 10),
        "daemon did not consume10 user CPU ticks",
    )?;
    require(files == 0, "shutdown must not add a flush")
}
struct Inputs {
    files: Vec<Bound>,
    absent: Vec<PathBuf>,
    hashes: BTreeMap<String, String>,
}
impl Inputs {
    fn new(options: &Options, worker: &Path) -> Result<Self> {
        let pins: BTreeMap<String, String> = serde_json::from_str(include_str!(
            "../../tests/fixtures/stats-runtime/source-pins.json"
        ))?;
        let mut result = Self {
            files: vec![],
            absent: vec![],
            hashes: BTreeMap::new(),
        };
        for (name, expected) in pins {
            let path = options.root.join(&name);
            if matches!(
                name.as_str(),
                "rust/tools/check_stats_runtime.py" | "rust/tools/check_stats_flood.py"
            ) && fs::symlink_metadata(&path)
                .is_err_and(|error| error.kind() == io::ErrorKind::NotFound)
            {
                result.absent.push(path);
                continue;
            }
            let file = Bound::open(&path)?;
            require(
                file.hash == expected,
                &format!("statistics runtime source changed: {name}"),
            )?;
            result.hashes.insert(name, file.hash.clone());
            result.files.push(file);
        }
        let file = Bound::open(worker)?;
        result.hashes.insert("checker".into(), file.hash.clone());
        result.files.push(file);
        Ok(result)
    }
    fn bind(&mut self, binaries: &Path, name: &str, expected: &str) -> Result<()> {
        let path = match name {
            "statsd-rs" => binaries.join("statsd-rs"),
            "stats_flood" => binaries.join("examples/stats_flood"),
            _ => return Err("unknown runtime worker artifact".into()),
        };
        self.verify()?;
        let file = Bound::open(&path)?;
        require(
            file.hash == expected,
            "runtime artifact changed before live admission",
        )?;
        if let Some(previous) = self.hashes.get(name) {
            require(
                previous == expected,
                "runtime artifact changed between signal cases",
            )?;
        }
        self.hashes.insert(name.into(), file.hash.clone());
        self.files.push(file);
        Ok(())
    }
    fn verify(&self) -> Result<()> {
        for path in &self.absent {
            require(
                fs::symlink_metadata(path)
                    .is_err_and(|error| error.kind() == io::ErrorKind::NotFound),
                "retired statistics source appeared during execution",
            )?;
        }
        for file in &self.files {
            file.verify()?;
        }
        Ok(())
    }
    fn attach(&self, value: &JsonValue) -> Result<JsonValue> {
        let mut text = value.to_json()?;
        require(text.pop() == Some('}'), "worker result is not an object")?;
        Ok(JsonValue::parse(&format!(
            "{text},\"input_sha256\":{}}}",
            serde_json::to_string(&self.hashes)?
        ))?)
    }
}
fn publish(path: &Path, value: &JsonValue, inputs: &Inputs, cancel: &AtomicBool) -> Result<()> {
    check(cancel)?;
    inputs.verify()?;
    let mut pending = tempfile::NamedTempFile::new_in(path.parent().ok_or("receipt parent")?)?;
    pending.write_all(value.to_json()?.as_bytes())?;
    inputs.verify()?;
    check(cancel)?;
    let held = pending
        .persist_noclobber(path)
        .map_err(|error| error.error)?;
    let result = inputs.verify().and_then(|()| check(cancel));
    if result.is_err() {
        if let (Ok(owned), Ok(now)) = (held.metadata(), fs::symlink_metadata(path)) {
            if (owned.dev(), owned.ino()) == (now.dev(), now.ino()) {
                let _ = fs::remove_file(path);
            }
        }
    }
    result
}
fn run_case(
    options: &Options,
    case: &str,
    worker: &Path,
    cancel: &AtomicBool,
    inputs: &mut Inputs,
) -> Result<JsonValue> {
    fs::create_dir_all(options.output.parent().ok_or("output parent")?)?;
    fs::create_dir(&options.output)?;
    // The global production endpoint is intentionally unchanged. Tests/cases
    // run serially, and any existing object must remain untouched.
    let namespace = Namespace::new()?;
    let endpoint =
        crate::qa_endpoint::EndpointOwner::new(Path::new("/tmp/stats"), &namespace.prefix)?;
    let mut command = Command::new(worker);
    command
        .arg("--stats-runtime-worker")
        .arg(case)
        .arg(&options.binaries)
        .arg(&options.output)
        .env("OPENPILOT_PREFIX", &namespace.prefix)
        .env("STATS_ENDPOINT_OWNER", namespace.endpoint.channel())
        .env("STATS_RUNTIME_ENDPOINT_OWNER", endpoint.channel())
        .env("HOME", options.output.join("home"))
        .env("PARAMS_ROOT", options.output.join("params"))
        .stderr(Stdio::from(File::create(
            options.output.join("worker.stderr"),
        )?));
    let mut child = Process::spawn(&mut command, cancel, true)?;
    let report = loop {
        let line = child.line(None).map_err(|error| {
            format!(
                "{error}: {}",
                fs::read_to_string(options.output.join("worker.stderr")).unwrap_or_default()
            )
        })?;
        let row = JsonValue::parse(&line)?;
        let kind = row.get("kind").ok_or("missing worker message kind")?;
        if kind.text_eq("bind") {
            let name = row
                .get("name")
                .and_then(|v| v.to_utf8())
                .ok_or("worker artifact name")?;
            let hash = row
                .get("sha256")
                .and_then(|v| v.to_utf8())
                .ok_or("worker artifact hash")?;
            inputs.bind(&options.binaries, &name, &hash)?;
            child.send("continue\n")?;
        } else if kind.text_eq("result") {
            break row.get("value").ok_or("missing worker observation")?;
        } else {
            return Err("unknown runtime worker message".into());
        }
    };
    child.close_input();
    let extra = child.output()?;
    require(extra.is_empty(), "unexpected runtime worker output")?;
    let code = child.wait(None)?;
    drop(child);
    require(
        code == 0,
        &format!(
            "runtime worker failed ({code}): {}",
            fs::read_to_string(options.output.join("worker.stderr"))?
        ),
    )?;
    inputs.verify()?;
    check(cancel)?;
    endpoint.verify()?;
    namespace.close()?;
    drop(endpoint);
    inputs.attach(&report)
}
pub fn run_runtime(options: &Options, worker: &Path) -> Result<JsonValue> {
    execute(options, worker, false, false, &AtomicBool::new(false))
}
pub fn run_flood(options: &Options, worker: &Path) -> Result<JsonValue> {
    execute(options, worker, true, false, &AtomicBool::new(false))
}
fn execute(
    options: &Options,
    worker: &Path,
    flood: bool,
    announce: bool,
    cancel: &AtomicBool,
) -> Result<JsonValue> {
    let mut inputs = Inputs::new(options, worker)?;
    let report = if flood {
        fs::create_dir_all(&options.output)?;
        let mut rows = vec![];
        for case in ["SIGINT", "SIGTERM"] {
            let nested = Options {
                root: options.root.clone(),
                binaries: options.binaries.clone(),
                output: options.output.join(case),
            };
            rows.push(run_case(&nested, case, worker, cancel, &mut inputs)?.to_json()?);
        }
        inputs.attach(&JsonValue::parse(&format!(
            "{{\"result\":\"PASS\",\"scenarios\":[{}]}}",
            rows.join(",")
        ))?)?
    } else {
        run_case(options, "runtime", worker, cancel, &mut inputs)?
    };
    inputs.verify()?;
    check(cancel)?;
    if announce {
        writeln!(
            io::stdout(),
            "{}",
            if flood {
                "SIGINT/SIGTERM stop a busy native drain while two native producers continue: PASS"
            } else {
                "production statsd-rs startup, real clock, file publication and SIGTERM PASS"
            }
        )?;
        check(cancel)?;
    }
    publish(
        &options.output.join(if flood {
            "comparison.json"
        } else {
            "result.json"
        }),
        &report,
        &inputs,
        cancel,
    )?;
    Ok(report)
}
/// Called only at the original subprocess spawn phase, after its output logs
/// have been opened. Both worker and outer owner then hold the admitted file.
fn bind(path: &Path, name: &str) -> Result<Bound> {
    let file = Bound::open(path)?;
    writeln!(
        io::stdout(),
        "{}",
        json!({"kind":"bind","name":name,"sha256":file.hash})
    )?;
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer)?;
    require(answer == "continue\n", "outer artifact admission closed")?;
    file.verify()?;
    Ok(file)
}
pub fn cli(flood: bool) -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 3 && args.first().is_some_and(|s| s == "--stats-collector") {
        return super::cli("daemon");
    }
    if args.len() == 4 && args.first().is_some_and(|s| s == "--stats-runtime-worker") {
        require(args.len() == 4, "invalid runtime worker arguments")?;
        let case = args[1].to_str().ok_or("worker case encoding")?;
        let value = live::run(
            case,
            Path::new(&args[2]),
            Path::new(&args[3]),
            &std::env::current_exe()?,
        )?;
        writeln!(
            io::stdout(),
            "{{\"kind\":\"result\",\"value\":{}}}",
            value.to_json()?
        )?;
        return Ok(());
    }
    // Original sys.argv unpacking: exactly two paths, including dashed names;
    // --help by itself is an arity error, not an argparse help flag.
    require(
        args.len() == 2,
        "usage: BIN_DIR OUTPUT (exactly two paths required)",
    )?;
    let options = Options {
        root: std::env::var_os("OPENPILOT_RUST_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")),
        binaries: resolve_non_strict(Path::new(&args[0]))?,
        output: resolve_non_strict(Path::new(&args[1]))?,
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let mut signals = Signals(vec![]);
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signals
            .0
            .push(signal_hook::flag::register(signal, cancel.clone())?);
    }
    execute(&options, &std::env::current_exe()?, flood, true, &cancel)?;
    Ok(())
}
