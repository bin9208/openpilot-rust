//! Actual continuous-daemon scenarios with independent unchanged-source captures.
//! This fixed controller does not link candidate statsd policy as its oracle.
use crate::{path_resolve::resolve_non_strict, seal::Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
pub(crate) mod identity;
pub mod interrupted;
pub(crate) mod process;
pub mod runtime;
mod worker;
use identity::Inputs;
use process::{check, Process};
pub struct Options {
    pub root: PathBuf,
    pub binaries: PathBuf,
    pub binding: PathBuf,
    pub output: PathBuf,
}
const FAILURES: [&str; 8] = [
    "utf8",
    "surrogate",
    "missing-directory",
    "permission",
    "collision",
    "dangling",
    "missing-param",
    "invalid-param",
];
fn expected(case: &str) -> Result<Value> {
    let bytes = include_bytes!("../tests/fixtures/stats-daemon/original.json");
    if format!("{:x}", Sha256::digest(bytes))
        != "6376c0a5e990c4040ffa115cde2fd357810c760a32a85dc2ba9f49c4eb2a57c1"
    {
        return Err("statistics original capture changed".into());
    }
    let value: Value = serde_json::from_slice(bytes)?;
    value
        .get(case)
        .cloned()
        .ok_or_else(|| "unknown statistics scenario".into())
}
struct Namespace {
    shm: tempfile::TempDir,
    socket: tempfile::TempDir,
    endpoint: crate::qa_endpoint::EndpointOwner,
    prefix: String,
}
impl Namespace {
    fn new() -> Result<Self> {
        let shm = tempfile::Builder::new()
            .prefix("msgq_stats-check-")
            .tempdir_in("/dev/shm")?;
        let prefix = shm
            .path()
            .file_name()
            .and_then(|v| v.to_str())
            .and_then(|v| v.strip_prefix("msgq_"))
            .ok_or("invalid namespace")?
            .to_owned();
        let log_endpoint = PathBuf::from(format!("/tmp/logmessage{prefix}"));
        let endpoint = crate::qa_endpoint::EndpointOwner::new(&log_endpoint, &prefix)?;
        let socket = tempfile::Builder::new()
            .prefix("stats-check-")
            .tempdir_in("/tmp")?;
        Ok(Self {
            shm,
            socket,
            endpoint,
            prefix,
        })
    }
    fn close(self) -> Result<()> {
        self.endpoint.verify()?;
        drop(self.endpoint);
        self.shm.close()?;
        self.socket.close()?;
        Ok(())
    }
}
pub fn run_case(options: &Options, case: &str, worker: &Path) -> Result<Value> {
    run_controlled(options, case, worker, &AtomicBool::new(false))
}
fn run_controlled(
    options: &Options,
    case: &str,
    worker: &Path,
    cancel: &AtomicBool,
) -> Result<Value> {
    let wanted = expected(case)?;
    let inputs = Inputs::open(&options.root, &options.binding, &options.binaries, worker)?;
    check(cancel)?;
    if let Some(parent) = options.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&options.output)?;
    let receipt = options.output.join("result.json");
    let result = (|| {
        let namespace = Namespace::new()?;
        let mut command = Command::new(worker);
        command
            .arg("--stats-daemon-worker")
            .arg(case)
            .arg(&options.binaries)
            .arg(&options.output)
            .arg(namespace.socket.path())
            .env("OPENPILOT_PREFIX", &namespace.prefix)
            .env("STATS_ENDPOINT_OWNER", namespace.endpoint.channel())
            .env("HOME", options.output.join("home"))
            .env("PARAMS_ROOT", options.output.join("params"))
            .stderr(Stdio::from(File::create(
                options.output.join("worker.stderr"),
            )?));
        let mut child = Process::spawn(&mut command, cancel, true)?;
        child.close_input();
        let bytes = child.output()?;
        let code = child.wait(None)?;
        drop(child);
        fs::write(options.output.join("worker.stdout"), &bytes)?;
        if code != 0 {
            return Err(format!(
                "statistics {case} worker failed ({code}): {}",
                fs::read_to_string(options.output.join("worker.stderr"))?
            )
            .into());
        }
        let actual: Value = serde_json::from_slice(&bytes)?;
        if actual != wanted {
            return Err(format!("statistics {case} result differs from unchanged original: expected {wanted}; observed {actual}").into());
        }
        inputs.verify()?;
        check(cancel)?;
        // Keep namespace ownership alive through all descendants; then clean before PASS.
        namespace.close()?;
        let result = json!({"result":"PASS","reference_kind":"captured unchanged original","case":case,"source":wanted,"native":actual,"input_sha256":inputs.hashes});
        fs::write(&receipt, serde_json::to_vec_pretty(&result)?)?;
        inputs.verify()?;
        check(cancel)?;
        Ok(result)
    })();
    if result.is_err() {
        let _ = fs::remove_file(receipt);
    }
    result
}
struct Signals(Vec<signal_hook::SigId>);
impl Drop for Signals {
    fn drop(&mut self) {
        for id in self.0.drain(..) {
            signal_hook::low_level::unregister(id);
        }
    }
}
pub fn cli(mode: &str) -> Result<()> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "--stats-collector") {
        if args.len() != 3 {
            return Err("invalid collector arguments".into());
        }
        return crate::managed_entry_collector_source::run(
            args[1].to_str().ok_or("endpoint encoding")?,
            Path::new(&args[2]),
        );
    }
    if args.first().is_some_and(|a| a == "--stats-daemon-worker") {
        if args.len() != 5 {
            return Err("invalid statistics worker arguments".into());
        }
        let result = worker::run(
            args[1].to_str().ok_or("scenario encoding")?,
            Path::new(&args[2]),
            Path::new(&args[3]),
            Path::new(&args[4]),
            &std::env::current_exe()?,
        )?;
        println!("{}", serde_json::to_string(&result)?);
        return Ok(());
    }
    if args.len() != 3 {
        return Err(
            "usage: check-stats-<daemon|failures|log-fault> BIN_DIR PARAMS_BINDING OUTPUT".into(),
        );
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let options = Options {
        root,
        binaries: resolve_non_strict(Path::new(&args[0]))?,
        binding: resolve_non_strict(Path::new(&args[1]))?,
        output: resolve_non_strict(Path::new(&args[2]))?,
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let mut signals = Signals(vec![]);
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signals
            .0
            .push(signal_hook::flag::register(signal, cancel.clone())?);
    }
    let binary = std::env::current_exe()?;
    if mode == "failures" {
        fs::create_dir_all(options.output.parent().ok_or("output parent missing")?)?;
        fs::create_dir(&options.output)?;
        let mut rows = vec![];
        for case in FAILURES {
            let nested = Options {
                root: options.root.clone(),
                binaries: options.binaries.clone(),
                binding: options.binding.clone(),
                output: options.output.join(case),
            };
            rows.push(run_controlled(&nested, case, &binary, &cancel)?);
        }
        check(&cancel)?;
        fs::write(
            options.output.join("comparison.json"),
            serde_json::to_vec_pretty(&json!({"result":"PASS","scenarios":rows}))?,
        )?;
    } else {
        let result = run_controlled(&options, mode, &binary, &cancel)?;
        fs::write(
            options.output.join("comparison.json"),
            serde_json::to_vec_pretty(&result)?,
        )?;
    }
    if cancel.load(Ordering::SeqCst) {
        let _ = fs::remove_file(options.output.join("comparison.json"));
        let _ = fs::remove_file(options.output.join(if mode == "failures" {
            "comparison.json"
        } else {
            "result.json"
        }));
        return Err("statistics comparison cancelled".into());
    }
    println!("PASS: native statistics {mode} matches unchanged captured source");
    Ok(())
}
