//! Real ZMQ checks against independently captured original StatLog contracts.
//! Candidate producer code never supplies the expected wire records.
use crate::{
    qa_process::{CommandSpec, NativeRunner},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs::{self, File, Metadata},
    io::{Read, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub struct Options {
    pub root: PathBuf,
    pub examples: PathBuf,
    pub output: PathBuf,
}
pub fn expected_messages(mode: &str) -> Result<Value> {
    Ok(match mode {
        "reconnect" => json!(["parent-before", "child-record", "parent-after"]),
        "drop" => json!(["parent-before", "parent-after"]),
        "pressure" => json!((0..1000)
            .map(|n| format!("pressure:{n}.0|g"))
            .collect::<Vec<_>>()),
        "types" => json!([
            "gpu_usage_percent:6|g",
            "memory_usage_percent:42|g",
            "fan_speed_percent_desired:65535|g",
            "screen_brightness_percent:0|g",
            "free_space_percent:62.099998474121094|g",
            "cpu0_usage_percent:-128|g",
            "cpu1_usage_percent:127|g",
            "cpu0_temperature:40.20000076293945|g",
            "integer_sample:6|sa",
            "float_sample:-0.0|sa"
        ]),
        _ => return Err("unknown StatLog fixture mode".into()),
    })
}
fn identity(m: &Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
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
struct Artifact {
    path: PathBuf,
    file: File,
    metadata: Metadata,
    sha256: String,
}
impl Artifact {
    fn open(path: &Path) -> Result<Self> {
        let fd = rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        let mut file = File::from(fd);
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err("StatLog checker requires a regular input file".into());
        }
        let mut hash = Sha256::new();
        let mut bytes = [0; 65536];
        loop {
            let count = file.read(&mut bytes)?;
            if count == 0 {
                break;
            }
            hash.update(&bytes[..count]);
        }
        let result = Self {
            path: path.to_owned(),
            file,
            metadata,
            sha256: format!("{:x}", hash.finalize()),
        };
        result.verify()?;
        Ok(result)
    }
    fn verify(&self) -> Result<()> {
        if identity(&self.metadata) != identity(&self.file.metadata()?)
            || identity(&self.metadata) != identity(&fs::metadata(&self.path)?)
        {
            return Err(format!("StatLog input changed: {}", self.path.display()).into());
        }
        Ok(())
    }
}
fn sources(root: &Path) -> Result<Vec<Artifact>> {
    let pins: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/stats-producer/source-pins.json"
    ))?;
    let mut files = Vec::new();
    for pin in pins.as_array().ok_or("invalid source pins")? {
        let name = pin["path"].as_str().ok_or("invalid source path")?;
        let path = root.join(name);
        let retired = matches!(
            name,
            "rust/tools/check_stats_producer.py" | "rust/tools/stats_source_producer.py"
        );
        if retired
            && fs::symlink_metadata(&path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
        {
            continue;
        }
        let input = Artifact::open(&path)?;
        if Some(input.sha256.as_str()) != pin["sha256"].as_str() {
            return Err(format!("StatLog source pin changed: {name}").into());
        }
        files.push(input);
    }
    Ok(files)
}
fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::SeqCst) {
        Err("StatLog comparison cancelled".into())
    } else {
        Ok(())
    }
}
pub fn run(options: &Options, worker: &Path) -> Result<Value> {
    run_controlled(options, worker, &AtomicBool::new(false))
}
fn run_controlled(options: &Options, worker: &Path, cancel: &AtomicBool) -> Result<Value> {
    let mut held = sources(&options.root)?;
    check_cancel(cancel)?;
    let checker = Artifact::open(worker)?;
    let checker_sha256 = checker.sha256.clone();
    held.push(checker);
    let mut binaries = serde_json::Map::new();
    for name in ["stats_fork", "stats_pressure", "stats_types"] {
        let artifact = Artifact::open(&options.examples.join(name))?;
        binaries.insert(name.into(), json!(artifact.sha256));
        held.push(artifact);
    }
    if let Some(parent) = options.output.parent() {
        fs::create_dir_all(parent)?;
    }
    // Exclusive ownership prevents a failed invocation from removing foreign receipts.
    fs::create_dir(&options.output)?;
    let receipt = options.output.join("result.json");
    let result = (|| {
        let mut rows = Vec::new();
        for mode in ["reconnect", "drop", "pressure", "types"] {
            check_cancel(cancel)?;
            // This owner survives a killed worker and removes its private socket.
            let socket_root = tempfile::Builder::new()
                .prefix("stats-check-")
                .tempdir_in("/tmp")?;
            let mut command = CommandSpec::new_os(
                vec![
                    worker.as_os_str().to_owned(),
                    "--stats-producer-worker".into(),
                    options.examples.as_os_str().to_owned(),
                    mode.into(),
                    socket_root.path().as_os_str().to_owned(),
                ],
                Duration::from_secs(30),
            );
            command.stdout_limit = 128 * 1024;
            command.stderr_limit = 1024 * 1024;
            let observed =
                NativeRunner.run_controlled(&command, Instant::now() + command.timeout, cancel)?;
            fs::write(
                options.output.join(format!("{mode}.stderr")),
                &observed.stderr,
            )?;
            fs::write(
                options.output.join(format!("{mode}.stdout")),
                &observed.stdout,
            )?;
            if observed.code != 0 {
                return Err(format!(
                    "StatLog {mode} worker failed ({}): {}",
                    observed.code,
                    String::from_utf8_lossy(&observed.stderr)
                )
                .into());
            }
            let row: Value = serde_json::from_slice(&observed.stdout)?;
            if row["mode"] != mode
                || row["exit"] != 0
                || row["messages"] != expected_messages(mode)?
            {
                return Err(format!("StatLog {mode} result differs from original").into());
            }
            rows.push(row);
        }
        for file in &held {
            file.verify()?;
        }
        check_cancel(cancel)?;
        let report = json!({"result":"PASS", "reference_kind":"captured unchanged original", "native":rows, "binary_sha256":binaries, "checker_sha256":checker_sha256});
        fs::write(&receipt, serde_json::to_vec_pretty(&report)?)?;
        check_cancel(cancel)?;
        for file in &held {
            file.verify()?;
        }
        Ok(report)
    })();
    if result.is_err() {
        let _ = fs::remove_file(receipt);
    }
    result
}
struct OwnedChild {
    child: Child,
    reaped: bool,
}
impl OwnedChild {
    fn done(&mut self) -> Result<Option<std::process::ExitStatus>> {
        let status = self.child.try_wait()?;
        if status.is_some() {
            self.reaped = true;
        }
        Ok(status)
    }
    fn ack(&mut self) -> Result<()> {
        let input = self
            .child
            .stdin
            .as_mut()
            .ok_or("StatLog child stdin missing")?;
        input.write_all(b"ack\n")?;
        input.flush()?;
        Ok(())
    }
    fn finish(&mut self, seconds: u64) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(seconds);
        loop {
            if let Some(status) = self.done()? {
                return if status.success() {
                    Ok(())
                } else {
                    Err(format!("StatLog child exited {status}").into())
                };
            }
            if Instant::now() >= deadline {
                return Err("StatLog child exit timeout".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn ready(&mut self) -> Result<()> {
        let stdout = self
            .child
            .stdout
            .as_mut()
            .ok_or("StatLog child stdout missing")?;
        let flags = rustix::fs::fcntl_getfl(&*stdout)?;
        rustix::fs::fcntl_setfl(&*stdout, flags | rustix::fs::OFlags::NONBLOCK)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut line = Vec::new();
        loop {
            let mut byte = [0];
            match stdout.read(&mut byte) {
                Ok(0) => return Err("StatLog pressure child ended before ready".into()),
                Ok(_) if byte[0] == b'\n' => {
                    if String::from_utf8(line)?.trim() != "ready" {
                        return Err("StatLog pressure readiness differs".into());
                    }
                    return Ok(());
                }
                Ok(_) => line.push(byte[0]),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => return Err(error.into()),
            }
            if line.len() > 65536 || Instant::now() >= deadline {
                return Err("StatLog pressure readiness timeout/limit".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        // The outer NativeRunner owns and terminates the complete worker group,
        // including any fork child. This inner owner never signals a reaped PID.
    }
}
fn worker(examples: &Path, mode: &str, root: &Path) -> Result<Value> {
    let expected = expected_messages(mode)?;
    let endpoint = format!("ipc://{}", root.join("socket").display());
    let context = zmq::Context::new();
    let receiver = context.socket(zmq::PULL)?;
    receiver.set_rcvtimeo(5000)?;
    if mode != "pressure" {
        receiver.bind(&endpoint)?;
    }
    let name = match mode {
        "reconnect" | "drop" => "stats_fork",
        "pressure" => "stats_pressure",
        "types" => "stats_types",
        _ => unreachable!(),
    };
    let mut command = Command::new(examples.join(name));
    command.arg(&endpoint);
    if matches!(mode, "reconnect" | "drop") {
        command.arg(mode);
    }
    let mut child = OwnedChild {
        child: command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?,
        reaped: false,
    };
    if mode == "pressure" {
        child.ready()?;
        receiver.bind(&endpoint)?;
    }
    let mut messages = Vec::new();
    for wanted in expected.as_array().ok_or("invalid expected wire records")? {
        let bytes = receiver.recv_bytes(0)?;
        let message = std::str::from_utf8(&bytes)?;
        if Some(message) != wanted.as_str() {
            return Err(format!(
                "StatLog {mode} message {} differs: {message:?}",
                messages.len()
            )
            .into());
        }
        messages.push(message.to_owned());
        if mode != "pressure" {
            child.ack()?;
        }
    }
    if mode == "pressure" {
        if receiver.poll(zmq::POLLIN, 100)? != 0 {
            return Err("StatLog pressure accepted more than original queue limit".into());
        }
        child.ack()?;
    }
    child.finish(if matches!(mode, "reconnect" | "drop") {
        10
    } else {
        5
    })?;
    receiver.unbind(&endpoint)?;
    Ok(json!({"mode":mode, "messages":messages, "exit":0}))
}
pub fn cli() -> Result<()> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|v| v == "--stats-producer-worker") {
        if args.len() != 4 {
            return Err("invalid StatLog worker arguments".into());
        }
        let report = worker(
            Path::new(&args[1]),
            args[2].to_str().ok_or("invalid StatLog mode")?,
            Path::new(&args[3]),
        )?;
        println!("{}", serde_json::to_string(&report)?);
        return Ok(());
    }
    if args.len() == 1 && matches!(args[0].to_str(), Some("--help" | "-h")) {
        println!("check-stats-producer EXAMPLES_DIR OUTPUT\nCompare real native ZMQ producers with captured original contracts.");
        return Ok(());
    }
    if args.len() != 2 {
        return Err("usage: check-stats-producer EXAMPLES_DIR OUTPUT".into());
    }
    let options = Options {
        root: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."),
        examples: crate::path_resolve::resolve_non_strict(Path::new(&args[0]))?,
        output: crate::path_resolve::resolve_non_strict(Path::new(&args[1]))?,
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let mut ids = Vec::new();
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        ids.push(signal_hook::flag::register(signal, Arc::clone(&cancel))?);
    }
    let result = run_controlled(&options, &std::env::current_exe()?, &cancel);
    for id in ids {
        signal_hook::low_level::unregister(id);
    }
    if cancel.load(Ordering::SeqCst) {
        if result.is_ok() {
            let _ = fs::remove_file(options.output.join("result.json"));
        }
        return Err("StatLog comparison cancelled".into());
    }
    result?;
    println!(
        "PASS: four native StatLog scenarios match independently captured original wire contracts"
    );
    Ok(())
}
