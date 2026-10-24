//! Receive-fault scenarios using a real link-wrapped daemon and sealed original captures.
use super::{check, identity::Inputs, worker::Collector, Namespace, Process, Signals};
use crate::{
    argparse_scan::{scan_os_with_positionals, OptionSpec},
    path_resolve::resolve_non_strict,
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs::{self, File},
    io::{self, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};
const CASES: [(&str, &str); 5] = [
    ("one", "4,0"),
    ("repeated", "4,4,4,0"),
    ("empty", "4,11,0"),
    ("invalid", "4,22"),
    ("closed", "4,88"),
];
pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub binding: PathBuf,
    pub preload: PathBuf,
    pub output: PathBuf,
    pub before: bool,
}
fn expected(case: &str) -> Result<Value> {
    let bytes = include_bytes!("../../tests/fixtures/stats-interrupted/original.json");
    if format!("{:x}", Sha256::digest(bytes))
        != "30af65083d9293acd0791f11d2ce40cb8eb611d2a7b4211dc82601d91e69020a"
    {
        return Err("interrupted-receive original capture changed".into());
    }
    let rows: Vec<Value> = serde_json::from_slice(bytes)?;
    rows.into_iter()
        .find(|r| r["case"] == case)
        .map(|r| r["result"].clone())
        .ok_or_else(|| "unknown interrupted-receive scenario".into())
}
pub fn compare_row(case: &str, row: &Value, before: bool) -> Result<()> {
    let original = expected(case)?;
    let valid = if before {
        row["exit"].as_i64().is_some_and(|v| v != 0)
            && row["files"] == json!([])
            && row["faults"] == json!([4])
    } else {
        *row == original
    };
    if valid {
        Ok(())
    } else {
        Err(format!("interrupted-receive {case} differs from original contract").into())
    }
}
pub fn run(options: &Options, worker: &Path) -> Result<Value> {
    run_controlled(options, worker, &AtomicBool::new(false), false)
}
fn run_controlled(
    options: &Options,
    worker: &Path,
    cancel: &AtomicBool,
    announce: bool,
) -> Result<Value> {
    let pins: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/stats-interrupted/source-pins.json"
    ))?;
    let inputs = Inputs::open_with(
        &options.root,
        &options.binding,
        &[
            ("checker", worker.into()),
            ("stats_daemon", options.binary.clone()),
            ("receive_preload", options.preload.clone()),
        ],
        Some(&pins),
    )?;
    check(cancel)?;
    if let Some(parent) = options.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&options.output)?;
    let receipt = options.output.join("result.json");
    let mut rows = vec![];
    for (case, _) in CASES {
        let output = options.output.join(case).join("native");
        fs::create_dir_all(&output)?;
        let namespace = Namespace::new()?;
        let mut command = Command::new(worker);
        command
            .arg("--stats-interrupted-worker")
            .arg(&options.binary)
            .arg(case)
            .arg(&output)
            .arg(namespace.socket.path())
            .arg(if options.before { "before" } else { "current" })
            .env("OPENPILOT_PREFIX", &namespace.prefix)
            .env("STATS_ENDPOINT_OWNER", namespace.endpoint.channel())
            .env("HOME", output.join("home"))
            .env("PARAMS_ROOT", output.join("params"))
            .stderr(Stdio::from(File::create(output.join("worker.stderr"))?));
        let mut child = Process::spawn(&mut command, cancel, true)?;
        child.close_input();
        let bytes = child.output()?;
        let code = child.wait(None)?;
        drop(child);
        fs::write(output.join("worker.stdout"), &bytes)?;
        if code != 0 {
            return Err(format!(
                "interrupted-receive {case} worker failed ({code}): {}",
                fs::read_to_string(output.join("worker.stderr"))?
            )
            .into());
        }
        let native: Value = serde_json::from_slice(&bytes)?;
        compare_row(case, &native, options.before)?;
        inputs.verify()?;
        check(cancel)?;
        namespace.close()?;
        rows.push(json!({"case":case,"source":expected(case)?,"native":native}));
    }
    let value = json!({"before":options.before,"reference_kind":"captured unchanged original","results":rows,"input_sha256":inputs.hashes});
    inputs.verify()?;
    check(cancel)?;
    // Final stdout can block. Do not publish a successful receipt until the
    // fallible write and its cancellation check have both completed.
    if announce {
        writeln!(
            io::stdout(),
            "{}",
            if options.before {
                "reproduced mismatch: native receiver exits on EINTR"
            } else {
                "PASS: interrupted receive matches unchanged captured source"
            }
        )?;
        check(cancel)?;
    }
    let mut pending = tempfile::NamedTempFile::new_in(&options.output)?;
    pending.write_all(&serde_json::to_vec_pretty(&value)?)?;
    inputs.verify()?;
    check(cancel)?;
    let published = pending.persist_noclobber(&receipt).map_err(|e| e.error)?;
    let completion = inputs.verify().and_then(|()| check(cancel));
    if completion.is_err() {
        if let (Ok(owned), Ok(now)) = (published.metadata(), fs::symlink_metadata(&receipt)) {
            if owned.dev() == now.dev() && owned.ino() == now.ino() {
                let _ = fs::remove_file(&receipt);
            }
        }
    }
    completion?;
    Ok(value)
}

fn require(value: bool, message: &str) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(message.to_owned().into())
    }
}
fn trace(path: &Path) -> Result<Vec<[i64; 5]>> {
    fs::read_to_string(path)?
        .lines()
        .map(|line| {
            let fields = line
                .split('\t')
                .map(str::parse)
                .collect::<std::result::Result<Vec<i64>, _>>()?;
            fields
                .try_into()
                .map_err(|_| "invalid receive trace column count".into())
        })
        .collect()
}
fn clock(child: &mut Process<'_>) -> Result<()> {
    require(
        child.line(Some(Duration::from_secs(5)))? == "clock",
        "clock acknowledgement differs",
    )
}
fn worker(binary: &Path, case: &str, output: &Path, socket: &Path, before: bool) -> Result<Value> {
    let errors = CASES
        .iter()
        .find(|(name, _)| *name == case)
        .ok_or("unknown receive scenario")?
        .1;
    let cancel = AtomicBool::new(false);
    let mut collector = Collector::start(output, &std::env::current_exe()?, &cancel)?;
    let metadata = output.join("metadata");
    fs::create_dir(&metadata)?;
    fs::write(
        metadata.join("build.json"),
        serde_json::to_vec(&json!({"channel":"fixture","openpilot":{"version":"v"}}))?,
    )?;
    let directory = output.join("stats");
    let endpoint = format!("ipc://{}", socket.join("receive").display());
    let trace_path = output.join("receive.tsv");
    let mut command = Command::new(binary);
    command
        .arg(&endpoint)
        .arg(&directory)
        .arg(&metadata)
        .env("ZMQ_TEST_RECV_ERRORS", errors)
        .env("ZMQ_TEST_RECV_TRACE", &trace_path)
        .stderr(Stdio::from(File::create(output.join("stderr.log"))?));
    let context = zmq::Context::new();
    let producer = context.socket(zmq::PUSH)?;
    producer.set_sndtimeo(3000)?;
    producer.set_linger(0)?;
    let mut child = Process::spawn(&mut command, &cancel, false)?;
    clock(&mut child)?;
    producer.connect(&endpoint)?;
    producer.send("retained:7|g", 0)?;
    child.send("100\n")?;
    let fatal = before || matches!(case, "invalid" | "closed");
    let exit;
    if fatal {
        require(
            child
                .line_or_eof(Some(Duration::from_secs(5)))?
                .is_none_or(|s| s.is_empty()),
            "fatal receive unexpectedly acknowledged clock",
        )?;
        exit = child.wait(Some(Duration::from_secs(5)))?;
        require(exit != 0, "fatal receive exited successfully")?;
    } else {
        clock(&mut child)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while !trace(&trace_path)?.iter().any(|row| row[3] > 0) {
            require(Instant::now() < deadline, "metric delivery timeout")?;
            child.send("100\n")?;
            clock(&mut child)?;
        }
        for tick in ["161\n", "162\n"] {
            child.send(tick)?;
            clock(&mut child)?;
        }
        child.close_input();
        exit = child.wait(Some(Duration::from_secs(5)))?;
        require(exit == 0, "successful receive exited unsuccessfully")?;
    }
    let rows = trace(&trace_path)?;
    require(
        rows.iter().all(|row| row[4] == i64::from(zmq::DONTWAIT)),
        "receive lost DONTWAIT",
    )?;
    let faults: Vec<i64> = rows
        .iter()
        .filter_map(|row| (row[1] != 0).then_some(row[1]))
        .collect();
    let mut paths = fs::read_dir(directory)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    let files = paths
        .into_iter()
        .map(fs::read_to_string)
        .collect::<std::io::Result<Vec<_>>>()?;
    if fatal {
        require(files.is_empty(), "fatal receive flushed statistics")?;
    } else {
        require(
            files.len() == 1
                && files[0].starts_with("gauge.retained,")
                && files[0].contains(" value=7.0,"),
            "retained metric flush differs",
        )?;
    }
    let result = json!({"exit":exit,"files":files,"faults":faults});
    compare_row(case, &result, before)?;
    drop(child);
    collector.stop()?;
    fs::write(
        output.join("result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    Ok(result)
}
pub fn cli() -> Result<()> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "--stats-collector") {
        return super::cli("interrupted");
    }
    if args
        .first()
        .is_some_and(|a| a == "--stats-interrupted-worker")
    {
        if args.len() != 6 || !matches!(args[5].to_str(), Some("before" | "current")) {
            return Err("invalid receive worker arguments".into());
        }
        let value = worker(
            Path::new(&args[1]),
            args[2].to_str().ok_or("scenario encoding")?,
            Path::new(&args[3]),
            Path::new(&args[4]),
            args[5] == "before",
        )?;
        println!("{}", serde_json::to_string(&value)?);
        return Ok(());
    }
    let parsed = scan_os_with_positionals(&args, &[OptionSpec::flag("--before", None)], 4)?;
    if parsed.help {
        writeln!(
            io::stdout(),
            "usage: check-stats-interrupted BINARY BINDING PRELOAD OUTPUT [--before]"
        )?;
        return Ok(());
    }
    let args = &parsed.positionals;
    let options = Options {
        root: std::env::var_os("OPENPILOT_RUST_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")),
        binary: resolve_non_strict(Path::new(&args[0]))?,
        binding: resolve_non_strict(Path::new(&args[1]))?,
        preload: resolve_non_strict(Path::new(&args[2]))?,
        output: resolve_non_strict(Path::new(&args[3]))?,
        before: parsed.values.contains_key("--before"),
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let mut signals = Signals(vec![]);
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signals
            .0
            .push(signal_hook::flag::register(signal, cancel.clone())?);
    }
    run_controlled(&options, &std::env::current_exe()?, &cancel, true)?;
    Ok(())
}
