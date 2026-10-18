//! Independent managed launcher/Sentry source policy in a fresh owned QA child.
//! External Python SDK selected fields are an explicitly labelled boundary;
//! actual candidate HTTP, collectors and lifecycle remain controller obligations.
use crate::seal::Result;
use log::Logger;
use openpilot_process_limits::source_title::SourceTitle;
use rustix::fs::{fcntl_getfl, fcntl_setfl, OFlags};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    backtrace::Backtrace,
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::{BufRead, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
mod log;
mod params;
mod policy;

#[derive(Debug)]
struct Error {
    kind: String,
    message: String,
    backtrace: String,
}
impl Error {
    fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
            backtrace: Backtrace::force_capture().to_string(),
        }
    }
    fn io(error: std::io::Error) -> Self {
        let kind = match error.raw_os_error() {
            Some(2) => "FileNotFoundError",
            Some(13 | 1) => "PermissionError",
            Some(20) => "NotADirectoryError",
            Some(21) => "IsADirectoryError",
            _ => "OSError",
        };
        Self::new(kind, error.to_string())
    }
    fn zmq(error: zmq::Error) -> Self {
        Self::new("ZMQError", error.to_string())
    }
    fn trace(&self) -> String {
        format!("{}: {}\n{}", self.kind, self.message, self.backtrace)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}
impl std::error::Error for Error {}
const SOURCES: &[(&str, &str)] = &[
    (
        "rust/tools/managed_entry_reference.py",
        "3f502d9c478e5f801f84b52ab5b613cb55f5d161b68434568e3942a72ac642d8",
    ),
    (
        "openpilot/system/manager/process.py",
        "c824eded96e88533f4d150a1b2ea0262cfd076cffea0692fad52ef4ba3bf5c8e",
    ),
    (
        "openpilot/common/logging_extra.py",
        "ce9b3b3d8498d76e1c4c0352a3e98e72759bb7f367ff8629c1329c46040595c3",
    ),
    (
        "openpilot/common/swaglog.py",
        "d37b623bd9418b72ffd3c4800a1da192c49856cb63bbbc1c09bb24a5ec6f8616",
    ),
    (
        "rust/tools/logging_producer_reference.py",
        "ac89bd78f4a2eed348317e374c4f21a9d2e28500abbeab10b50313ef3677a536",
    ),
    (
        "rust/tools/check_managed_entry.py",
        "89d2ce945fe94955d9b764f6a71434ac02298713a8983513bb9990318e6acc44",
    ),
    (
        "openpilot/common/params_pyx.pyx",
        "a8cc5e5296dd38a012c55b0867c4563f8e320d25960260d897675dd90f0ea544",
    ),
];
fn retired_helper(path: &str) -> bool {
    matches!(
        path,
        "rust/tools/check_managed_entry.py" | "rust/tools/managed_entry_reference.py"
    )
}
/// Runtime inputs remain mandatory. Only actual absence of the two retired
/// managed helpers is allowed; present changed/dangling aliases still fail.
pub fn sources(repository: &Path) -> Result<BTreeMap<String, String>> {
    let mut sources = crate::tombstone_source::sources(repository)?;
    for &(path, expected) in SOURCES {
        if retired_helper(path) {
            match fs::symlink_metadata(repository.join(path)) {
                Ok(_) => (),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            }
        }
        let actual = format!("{:x}", Sha256::digest(fs::read(repository.join(path))?));
        if actual != expected {
            return Err(format!("changed managed-entry original source: {path}").into());
        }
        sources.insert(path.into(), actual);
    }
    Ok(sources)
}
/// Compiled identities of only the actually absent retired helpers. Validate
/// the complete live roster first, so this is never an absence/dangling escape.
pub fn historical_sources(repository: &Path) -> Result<BTreeMap<String, String>> {
    let live = sources(repository)?;
    Ok(SOURCES
        .iter()
        .filter(|(path, _)| retired_helper(path) && !live.contains_key(*path))
        .map(|(path, hash)| ((*path).into(), (*hash).into()))
        .collect())
}

fn field<'a>(request: &'a Value, key: &str) -> Result<&'a str> {
    request[key]
        .as_str()
        .ok_or_else(|| format!("managed source {key} is not text").into())
}
fn line(output: &mut dyn Write, value: &Value) -> std::result::Result<(), Error> {
    serde_json::to_writer(&mut *output, value)
        .map_err(|error| Error::new("ValueError", error.to_string()))?;
    output.write_all(b"\n").map_err(Error::io)?;
    output.flush().map_err(Error::io)
}
// run is a fresh child stdin protocol. Nonblocking stdin lets the same
// caller-supplied BufRead retain partial rows while observing actual SIGINT.
// It avoids a reader thread or swallowing a signal during a blocking read.
struct StdinMode(OFlags);
impl StdinMode {
    fn nonblocking() -> Result<Self> {
        let input = std::io::stdin();
        let flags = fcntl_getfl(&input)?;
        fcntl_setfl(&input, flags | OFlags::NONBLOCK)?;
        Ok(Self(flags))
    }
}
impl Drop for StdinMode {
    fn drop(&mut self) {
        let _ = fcntl_setfl(std::io::stdin(), self.0);
    }
}
fn acknowledge(
    input: &mut dyn BufRead,
    interrupted: &AtomicBool,
) -> std::result::Result<(), Error> {
    let mut command = Vec::new();
    loop {
        if interrupted.load(Ordering::Relaxed) {
            return Err(Error::new("KeyboardInterrupt", "source stdin interrupted"));
        }
        match input.read_until(b'\n', &mut command) {
            Ok(0) => break,
            Ok(_) if command.last() == Some(&b'\n') => break,
            Ok(_) => (),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(Error::io(error)),
        }
        thread::sleep(Duration::from_millis(2));
    }
    if interrupted.load(Ordering::Relaxed) {
        return Err(Error::new("KeyboardInterrupt", "source stdin interrupted"));
    }
    if std::str::from_utf8(&command).ok().map(str::trim) != Some("continue") {
        return Err(Error::new(
            "AssertionError",
            "source expected continue acknowledgement",
        ));
    }
    Ok(())
}
fn comm() -> std::result::Result<String, Error> {
    Ok(fs::read_to_string("/proc/self/comm")
        .map_err(Error::io)?
        .trim()
        .to_owned())
}
fn cmdline() -> std::result::Result<String, Error> {
    Ok(
        String::from_utf8_lossy(&fs::read("/proc/self/cmdline").map_err(Error::io)?)
            .replace('\0', " "),
    )
}
struct SignalGuard(signal_hook::SigId);
impl Drop for SignalGuard {
    fn drop(&mut self) {
        signal_hook::low_level::unregister(self.0);
    }
}
enum Failure {
    Interrupt,
    Ordinary(Error),
}
impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        if error.kind == "KeyboardInterrupt" {
            Self::Interrupt
        } else {
            Self::Ordinary(error)
        }
    }
}
// This source factory owns the live publisher used by the body. Reset replaces
// the factory and its registry; it is not an echoed context_ready fixture flag.
struct BodyContext {
    publisher: Option<openpilot_msgq::Publisher>,
}
impl BodyContext {
    fn empty() -> Box<Self> {
        Box::new(Self { publisher: None })
    }
    fn initialized() -> std::result::Result<Box<Self>, Error> {
        Ok(Box::new(Self {
            publisher: Some(
                openpilot_msgq::Publisher::for_runtime("managedEntryBody", 1024 * 1024)
                    .map_err(|error| Error::new("RuntimeError", error.to_string()))?,
            ),
        }))
    }
}
struct Worker<'a> {
    request: &'a Value,
    root: PathBuf,
    process: &'a str,
    daemon: &'a str,
    logger: Logger,
    sdk: policy::Sdk,
    title: SourceTitle,
    context: Box<BodyContext>,
    interrupted: Arc<AtomicBool>,
    steps: Vec<Value>,
    stage: &'static str,
    initial: bool,
}
impl Worker<'_> {
    fn missing(&self) -> std::result::Result<Vec<u8>, Error> {
        fs::read(self.root.join("missing-input")).map_err(Error::io)
    }
    fn gate(&self, key: &str) -> std::result::Result<(), Failure> {
        match self.request.get(key).and_then(Value::as_str) {
            Some("interrupt") => Err(Failure::Interrupt),
            Some("error") => {
                self.missing()?;
                Ok(())
            }
            None => Ok(()),
            Some(mode) => Err(Error::new("AssertionError", mode).into()),
        }
    }
    fn launch(
        &mut self,
        input: &mut dyn BufRead,
        output: &mut dyn Write,
    ) -> std::result::Result<(), Failure> {
        self.stage = "Prepare";
        self.steps.push(json!({"step":"prepare"}));
        self.gate("prepare").inspect_err(|failure| {
            self.initial = matches!(failure, Failure::Ordinary(_));
        })?;
        let prepared = fs::read(self.root.join("prepared-input"))
            .map_err(Error::io)
            .inspect_err(|_| self.initial = true)?;
        self.stage = "Name";
        if self.process.as_bytes().contains(&0) {
            self.initial = true;
            return Err(Error::new("ValueError", "embedded null character").into());
        }
        self.title
            .set(self.process.as_bytes())
            .map_err(Error::io)
            .inspect_err(|_| self.initial = true)?;
        self.stage = "ResetContext";
        self.steps
            .push(json!({"step":"reset_context","comm":comm()?}));
        self.gate("reset").inspect_err(|failure| {
            self.initial = matches!(failure, Failure::Ordinary(_));
        })?;
        let replacement = BodyContext::initialized().inspect_err(|_| self.initial = true)?;
        let previous = std::mem::replace(&mut self.context, replacement);
        let replaced = !std::ptr::eq(previous.as_ref(), self.context.as_ref());
        line(
            output,
            &json!({"context_ready":true,"context_replaced":replaced}),
        )
        .inspect_err(|_| self.initial = true)?;
        acknowledge(input, &self.interrupted)
            .inspect_err(|error| self.initial = error.kind != "KeyboardInterrupt")?;
        drop(previous);
        self.logger.bind(self.daemon);
        self.stage = "Tag";
        self.sdk
            .tag("daemon", json!(self.daemon))
            .inspect_err(|_| self.initial = true)?;
        self.stage = "Body";
        // The original decodes while constructing the step before its body try.
        let prepared = String::from_utf8(prepared)
            .map_err(|error| Error::new("UnicodeDecodeError", error.to_string()))?;
        self.steps.push(json!({"step":"body","prepared":prepared}));
        let result = self.body(output);
        if matches!(result, Err(Failure::Ordinary(_))) {
            self.initial = true;
        }
        result
    }
    fn body(&mut self, output: &mut dyn Write) -> std::result::Result<(), Failure> {
        self.logger.emit(20, "managed body entered", None)?;
        self.context
            .publisher
            .as_mut()
            .expect("publisher created by reset")
            .send(b"managed body IPC")
            .map_err(|error| Error::new("RuntimeError", error.to_string()))?;
        if self.request["close_logger"] == true {
            self.logger.close();
        }
        if self.request["params_fault"] == "put" {
            fs::set_permissions(self.root.join("params"), fs::Permissions::from_mode(0o500))
                .map_err(Error::io)?;
        }
        match self.request["body"].as_str() {
            Some("return") => Ok(()),
            Some("error") => {
                self.missing()?;
                Ok(())
            }
            Some("chain") => {
                self.missing().map_err(|error| {
                    Error::new(
                        "RuntimeError",
                        format!("failed to load managed body fixture; caused by {error}"),
                    )
                })?;
                Ok(())
            }
            Some("interrupt") => Err(Failure::Interrupt),
            Some("signal") => {
                line(output, &json!({"waiting_for_sigint":true}))?;
                while !self.interrupted.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(Failure::Interrupt)
            }
            _ => Err(Error::new("AssertionError", "unknown managed source body").into()),
        }
    }
}
/// Run once in a fresh owned source child, never inline in the controller.
/// Real process naming, signals and native IPC are intentionally process-local.
pub fn run(
    request: &Value,
    environment: &BTreeMap<OsString, OsString>,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<i32> {
    let root = PathBuf::from(field(request, "root")?);
    let prefix = field(request, "prefix")?;
    let process = field(request, "process")?;
    let daemon = field(request, "daemon")?;
    // Claim before foreign worker threads start. Title bytes change only at Name.
    let title = SourceTitle::claim()?;
    let _stdin_mode = StdinMode::nonblocking()?;
    std::env::set_var("OPENPILOT_PREFIX", prefix);
    let mut environment = environment.clone();
    environment.insert("OPENPILOT_PREFIX".into(), prefix.into());
    let interrupted = Arc::new(AtomicBool::new(false));
    let _signal = SignalGuard(signal_hook::flag::register(
        signal_hook::consts::SIGINT,
        Arc::clone(&interrupted),
    )?);
    let logger = Logger::new(field(request, "endpoint")?, request.get("global_daemon"))?;
    let sdk = policy::Sdk::new(&root, prefix, request, &environment, &interrupted)?;
    logger.emit(10, "managed-entry-ready", None)?;
    line(
        output,
        &json!({"ready":true,"reporting_enabled":sdk.enabled}),
    )?;
    acknowledge(input, &interrupted)?;
    let mut worker = Worker {
        request,
        root,
        process,
        daemon,
        logger,
        sdk,
        title,
        context: BodyContext::empty(),
        interrupted,
        steps: Vec::new(),
        stage: "Prepare",
        initial: false,
    };
    let outcome = match worker.launch(input, output) {
        Ok(()) => json!({"kind":"returned"}),
        Err(Failure::Interrupt) => {
            // Python consumed this KeyboardInterrupt before returning from the
            // launcher. A later acknowledgement waits for a new signal only.
            worker.interrupted.store(false, Ordering::Relaxed);
            match worker
                .logger
                .emit(30, &format!("child {process} got SIGINT"), None)
            {
                Ok(()) => {
                    json!({"kind": if matches!(request["body"].as_str(),Some("interrupt"|"signal")) || request["prepare"]=="interrupt" || request["reset"]=="interrupt" {"interrupted"}else{"returned"}})
                }
                Err(error) => {
                    json!({"kind":"interrupt_log_failed","stage":worker.stage,"exception_type":error.kind,"message":error.message,"traceback":error.trace()})
                }
            }
        }
        Err(Failure::Ordinary(error)) => {
            let params = worker.root.join(
                if request["params_fault"] == "open" && worker.stage == "Body" {
                    "params-blocked"
                } else {
                    "params"
                },
            );
            let reported = worker.sdk.report(&worker.logger, &error, &params);
            let kind = if !worker.initial {
                "interrupt_log_failed"
            } else if reported.is_ok() {
                "raised"
            } else {
                "reporting_failed"
            };
            let final_error = reported.err().unwrap_or(error);
            json!({"kind":kind,"stage":worker.stage,"exception_type":final_error.kind,"message":final_error.message,"traceback":final_error.trace()})
        }
    };
    let success = matches!(outcome["kind"].as_str(), Some("returned" | "interrupted"));
    line(
        output,
        &json!({"outcome":outcome,"steps":worker.steps,"sdk_calls":worker.sdk.calls,"comm":comm()?,"cmdline":cmdline()?,"external_sdk_boundary":{"kind":"captured-original-sdk-2.55.0-selected-fields","events":worker.sdk.captured}}),
    )?;
    acknowledge(input, &worker.interrupted)?;
    worker.logger.close();
    Ok(if success { 0 } else { 1 })
}
