//! Source-derived installed-daemon failure checks using actual native children.
//! No candidate managed-entry or collector policy supplies expected outcomes.
use crate::{
    argparse_scan::ArgumentError,
    path_resolve::resolve_non_strict,
    qa_process::{CommandSpec, NativeRunner},
    seal::{digest, Result},
};
use rustix::process::{kill_process_group, waitid, Pid, Signal, WaitId, WaitIdOptions};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs::{self, File},
    io::Read,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{FileTypeExt, MetadataExt},
        process::{CommandExt, ExitStatusExt},
    },
    path::{Path, PathBuf},
    process::{Child, Command as ProcessCommand},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
const SOURCE: &str = "rust/tools/check_managed_runtime.py";
const SOURCE_SHA: &str = "a0cb645be563a6490c6df1c660d5fd764cb5ef5804158b82741e147fc87ac473";
#[derive(Clone, Copy, Debug)]
pub struct Case {
    pub binary: &'static str,
    pub daemon: &'static str,
    pub error_type: &'static str,
}
const CASES: [Case; 7] = [
    Case {
        binary: "openpilot-controlsd",
        daemon: "controlsd",
        error_type: "openpilot_controlsd::",
    },
    Case {
        binary: "openpilot-radard",
        daemon: "radard",
        error_type: "openpilot_radard::",
    },
    Case {
        binary: "openpilot-plannerd",
        daemon: "plannerd",
        error_type: "openpilot_plannerd::",
    },
    Case {
        binary: "openpilot-paramsd",
        daemon: "paramsd",
        error_type: "openpilot_paramsd::",
    },
    Case {
        binary: "openpilot-proclogd-runtime",
        daemon: "proclogd",
        error_type: "runtime::EntryError",
    },
    Case {
        binary: "openpilot-uploader",
        daemon: "uploader",
        error_type: "openpilot_uploader::",
    },
    Case {
        binary: "openpilot-bluetoothd",
        daemon: "carrot_bluetooth",
        error_type: "openpilot_bluetooth::",
    },
];
pub fn cases() -> &'static [Case] {
    &CASES
}
#[derive(Debug, Clone)]
pub struct Options {
    pub binaries: PathBuf,
    pub output: PathBuf,
    pub daemon: Option<String>,
    pub inherited_context: bool,
    pub environment: BTreeMap<OsString, OsString>,
}
#[derive(Debug)]
pub enum Command {
    Help,
    Run(Options),
}
fn text(arg: &OsStr) -> String {
    crate::python_fs::codepoints(arg)
        .into_iter()
        .map(|p| {
            char::from_u32(p)
                .map(|c| c.to_string())
                .unwrap_or_else(|| format!("\\u{p:04x}"))
        })
        .collect()
}
fn repr(arg: &OsStr) -> String {
    crate::plot_samples_source::text_repr(&crate::python_fs::codepoints(arg))
}
fn argument(text: impl Into<String>) -> Box<dyn std::error::Error> {
    Box::new(ArgumentError(text.into()))
}
const FLAGS: [&str; 4] = [
    "--binaries",
    "--output",
    "--daemon",
    "--omit-inherited-context",
];
fn classify(arg: &OsStr) -> Result<Option<&'static str>> {
    let raw = arg.as_bytes();
    let name = raw.split(|b| *b == b'=').next().unwrap();
    if name == b"-h" || (name.starts_with(b"-h") && !name.starts_with(b"--")) {
        return Ok(Some("--help"));
    }
    let choices = FLAGS.iter().copied().chain(["--help"]);
    let matches = choices
        .filter(|s| {
            s.as_bytes() == name || (name.starts_with(b"--") && s.as_bytes().starts_with(name))
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(None),
        [one] => Ok(Some(one)),
        _ => Err(argument(format!(
            "ambiguous option: {} could match {}",
            text(OsStr::from_bytes(name)),
            matches.join(", ")
        ))),
    }
}
fn option(arg: &OsStr) -> bool {
    let raw = arg.as_bytes();
    let value = text(arg);
    let value = value.strip_suffix('\n').unwrap_or(&value);
    let negative = value.strip_prefix('-').is_some_and(|body| {
        if let Some((before, after)) = body.split_once('.') {
            !after.is_empty()
                && crate::argparse_scan::decimal(before)
                && crate::argparse_scan::decimal(after)
        } else {
            !body.is_empty() && crate::argparse_scan::decimal(body)
        }
    });
    raw != b"-" && raw.starts_with(b"-") && !raw.contains(&b' ') && !negative
}
pub fn parse(
    args: &[OsString],
    cwd: &Path,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<Command> {
    let mut literal = false;
    let mut classified = Vec::new();
    for arg in args {
        if arg == "--" {
            literal = true;
            classified.push(None);
        } else {
            classified.push(if literal { None } else { classify(arg)? });
        }
    }
    let mut values: BTreeMap<&str, OsString> = BTreeMap::new();
    let mut extras = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let Some(flag) = classified[i] else {
            extras.push(text(&args[i]));
            i += 1;
            continue;
        };
        let raw = args[i].as_bytes();
        let inline = raw
            .iter()
            .position(|b| *b == b'=')
            .map(|at| OsString::from_vec(raw[at + 1..].to_vec()));
        if flag == "--help" || flag == "--omit-inherited-context" {
            if let Some(value) = inline {
                return Err(argument(format!(
                    "argument {}: ignored explicit argument {}",
                    if flag == "--help" { "-h/--help" } else { flag },
                    repr(&value)
                )));
            }
            if flag == "--help" {
                return Ok(Command::Help);
            }
            values.insert(flag, OsString::new());
        } else {
            let value = if let Some(value) = inline {
                value
            } else {
                i += 1;
                args.get(i)
                    .filter(|v| classified[i].is_none() && !option(v))
                    .cloned()
                    .ok_or_else(|| argument(format!("argument {flag}: expected one argument")))?
            };
            if flag == "--daemon" && !CASES.iter().any(|c| value == c.daemon) {
                return Err(argument(format!(
                    "argument --daemon: invalid choice: {} (choose from {})",
                    repr(&value),
                    CASES
                        .iter()
                        .map(|c| format!("'{}'", c.daemon))
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
            values.insert(flag, value);
        }
        i += 1;
    }
    let missing = ["--binaries", "--output"]
        .into_iter()
        .filter(|key| !values.contains_key(key))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(argument(format!(
            "the following arguments are required: {}",
            missing.join(", ")
        )));
    }
    if !extras.is_empty() {
        return Err(argument(format!(
            "unrecognized arguments: {}",
            extras.join(" ")
        )));
    }
    Ok(Command::Run(Options {
        binaries: resolve_non_strict(&cwd.join(&values["--binaries"]))?,
        output: resolve_non_strict(&cwd.join(&values["--output"]))?,
        daemon: values
            .get("--daemon")
            .map(|v| v.to_str().expect("validated daemon").to_owned()),
        inherited_context: !values.contains_key("--omit-inherited-context"),
        environment: environment.clone(),
    }))
}
/// Only the exact retired checker may be absent; present aliases must resolve
/// to the historical bytes, while dangling symlinks are not actual absence.
pub fn sources(repository: &Path) -> Result<BTreeMap<String, String>> {
    let path = repository.join(SOURCE);
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
        Ok(_) => {
            if digest(&path)? != SOURCE_SHA {
                return Err("managed-runtime original checker changed".into());
            }
        }
    }
    Ok(BTreeMap::from([(SOURCE.into(), SOURCE_SHA.into())]))
}
#[derive(Debug)]
pub struct Interrupted(pub i32);
impl std::fmt::Display for Interrupted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "managed-runtime interrupted by signal {}", self.0)
    }
}
impl std::error::Error for Interrupted {}
struct Cancel {
    flag: Arc<AtomicBool>,
    signal: Arc<AtomicUsize>,
    handlers: Vec<signal_hook::SigId>,
}
impl Cancel {
    fn new() -> Result<Self> {
        let mut this = Self {
            flag: Arc::new(AtomicBool::new(false)),
            signal: Arc::new(AtomicUsize::new(0)),
            handlers: Vec::new(),
        };
        for s in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            this.handlers.push(signal_hook::flag::register_usize(
                s,
                Arc::clone(&this.signal),
                s as usize,
            )?);
            this.handlers
                .push(signal_hook::flag::register(s, Arc::clone(&this.flag))?);
        }
        Ok(this)
    }
    fn check(&self) -> Result<()> {
        if self.flag.load(Ordering::Relaxed) {
            Err(Box::new(Interrupted(
                self.signal.load(Ordering::Relaxed) as i32
            )))
        } else {
            Ok(())
        }
    }
    fn sleep(&self, duration: Duration) -> Result<()> {
        let end = Instant::now() + duration;
        loop {
            self.check()?;
            let now = Instant::now();
            if now >= end {
                return Ok(());
            }
            std::thread::sleep((end - now).min(Duration::from_millis(10)));
        }
    }
}
impl Drop for Cancel {
    fn drop(&mut self) {
        for h in self.handlers.drain(..) {
            signal_hook::low_level::unregister(h);
        }
    }
}
fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.to_owned().into())
    }
}
struct Collector {
    child: Option<Child>,
    pid: Pid,
}
impl Collector {
    fn spawn(command: &mut ProcessCommand) -> Result<Self> {
        let child = command.process_group(0).spawn()?;
        let pid = Pid::from_child(&child);
        Ok(Self {
            child: Some(child),
            pid,
        })
    }
    fn exited(&self) -> Result<bool> {
        match waitid(
            WaitId::Pid(self.pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        ) {
            Ok(v) => Ok(v.is_some()),
            Err(rustix::io::Errno::INTR) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
    fn stop(&mut self, cancel: &Cancel) -> Result<i32> {
        rustix::process::kill_process(self.pid, Signal::TERM)?;
        let end = Instant::now() + Duration::from_secs(10);
        while !self.exited()? {
            cancel.check()?;
            require(
                Instant::now() < end,
                "owned collector did not stop within 10 seconds",
            )?;
            cancel.sleep(Duration::from_millis(5))?;
        }
        let _ = kill_process_group(self.pid, Signal::KILL);
        let status = self
            .child
            .take()
            .ok_or("collector already reaped")?
            .wait()?;
        Ok(status
            .code()
            .unwrap_or_else(|| -status.signal().unwrap_or(1)))
    }
}
impl Drop for Collector {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // Reserve the leader until its group's final signal.
            let _ = kill_process_group(self.pid, Signal::KILL);
            let _ = child.wait();
        }
    }
}
struct Namespace {
    directory: tempfile::TempDir,
    prefix: String,
    endpoint: PathBuf,
    // Keep the pathname inode allocated after collector closure/unlink. A
    // remembered dev/ino pair alone can match a recycled foreign endpoint.
    socket_identity: Option<File>,
}
impl Namespace {
    fn new() -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("msgq_-managed-runtime-")
            .tempdir_in("/dev/shm")?;
        let prefix = directory
            .path()
            .file_name()
            .and_then(OsStr::to_str)
            .and_then(|s| s.strip_prefix("msgq_"))
            .ok_or("invalid owned prefix")?
            .to_owned();
        let endpoint = PathBuf::from(format!("/tmp/logmessage{prefix}"));
        match fs::symlink_metadata(&endpoint) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            _ => return Err("owned endpoint already exists".into()),
        }
        Ok(Self {
            directory,
            prefix,
            endpoint,
            socket_identity: None,
        })
    }
    fn observe(&mut self) {
        if self.socket_identity.is_some() {
            return;
        }
        if let Ok(fd) = rustix::fs::open(
            &self.endpoint,
            rustix::fs::OFlags::PATH | rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        ) {
            let file = File::from(fd);
            if file
                .metadata()
                .is_ok_and(|meta| meta.file_type().is_socket())
            {
                self.socket_identity = Some(file);
            }
        }
    }
}
impl Drop for Namespace {
    fn drop(&mut self) {
        if let (Some(expected), Ok(meta)) = (
            self.socket_identity
                .as_ref()
                .and_then(|file| file.metadata().ok()),
            fs::symlink_metadata(&self.endpoint),
        ) {
            if meta.file_type().is_socket()
                && (expected.dev(), expected.ino()) == (meta.dev(), meta.ino())
            {
                let _ = fs::remove_file(&self.endpoint);
            }
        }
    }
}
fn executable_digest(path: &Path) -> Result<String> {
    // Validate the opened descriptor, not metadata followed by another open:
    // replacing a pathname with a FIFO must not block preflight. Non-regular
    // inputs still reach the original exec failure phase via the caller's .ok().
    let fd = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?;
    let mut file = File::from(fd);
    require(
        file.metadata()?.is_file(),
        "executable input is not a regular file",
    )?;
    let mut hash = Sha256::new();
    let mut bytes = [0; 65536];
    loop {
        let count = file.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        hash.update(&bytes[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn command(
    binary: &Path,
    args: &[&str],
    environment: &BTreeMap<OsString, OsString>,
) -> CommandSpec {
    let mut spec = CommandSpec::new_os(
        std::iter::once(binary.as_os_str().to_owned())
            .chain(args.iter().map(OsString::from))
            .collect(),
        Duration::from_secs(15),
    );
    spec.set_os_env(environment.clone());
    spec.env_remove = std::env::vars_os()
        .filter_map(|(key, _)| (!environment.contains_key(&key)).then_some(key))
        .collect();
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    spec
}
fn capture(spec: &CommandSpec, cancel: &Cancel) -> Result<crate::qa_process::Captured> {
    let value =
        NativeRunner.run_controlled(spec, Instant::now() + Duration::from_secs(15), &cancel.flag);
    cancel.check()?;
    value
}
fn case_inner(options: &Options, case: &Case, cancel: &Cancel) -> Result<Value> {
    if let Some(parent) = options.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&options.output)?;
    let temporary = tempfile::Builder::new()
        .prefix("managed-runtime-")
        .tempdir()?;
    let root = temporary.path();
    fs::write(
        root.join("build.json"),
        b"{\"openpilot\": {\"git_origin\": \"fixture/owned\"}}",
    )?;
    let mut namespace = Namespace::new()?;
    let log_root = options.output.join("logs");
    let binary = options.binaries.join(case.binary);
    let collector_binary = options.binaries.join("openpilot-logmessaged");
    // Failed input reads are deferred to their original spawn/hash phase.
    let binary_before = executable_digest(&binary).ok();
    let collector_before = executable_digest(&collector_binary).ok();
    let mut environment = options.environment.clone();
    for (key, value) in [
        ("OPENPILOT_PREFIX", OsString::from(&namespace.prefix)),
        ("OPENPILOT_BASEDIR", root.as_os_str().to_owned()),
        ("PARAMS_ROOT", root.join("params").into_os_string()),
        ("RUST_BACKTRACE", "1".into()),
        ("MANAGER_DAEMON", case.daemon.into()),
        (
            "OPENPILOT_LOG_CONTEXT",
            r#"{"dongle_id": "owned-device", "version": "owned-version"}"#.into(),
        ),
    ] {
        environment.insert(key.into(), value);
    }
    if !options.inherited_context {
        environment.remove(OsStr::new("OPENPILOT_LOG_CONTEXT"));
    }
    let mut collector_env = environment.clone();
    collector_env.remove(OsStr::new("MANAGER_DAEMON"));
    collector_env.remove(OsStr::new("OPENPILOT_LOG_CONTEXT"));
    let log = File::create(options.output.join("collector.log"))?;
    let mut collector = Collector::spawn(
        ProcessCommand::new(&collector_binary)
            .arg("--log-root")
            .arg(&log_root)
            .env_clear()
            .envs(&collector_env)
            .stdout(log.try_clone()?)
            .stderr(log),
    )?;
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        cancel.check()?;
        namespace.observe();
        if namespace.endpoint.exists()
            && ["logMessage", "errorLogMessage"]
                .iter()
                .all(|name| namespace.directory.path().join(name).exists())
        {
            break;
        }
        require(
            !collector.exited()? && Instant::now() < end,
            "owned collector did not initialize",
        )?;
        cancel.sleep(Duration::from_millis(10))?;
    }
    let failed = capture(
        &command(&binary, &["--invalid-managed-fixture"], &environment),
        cancel,
    )?;
    fs::write(options.output.join("stdout.txt"), &failed.stdout)?;
    fs::write(options.output.join("stderr.txt"), &failed.stderr)?;
    require(failed.code == 1, "daemon invalid fixture did not exit 1")?;
    require(
        failed.stderr.windows(6).any(|v| v == b"crash\n"),
        "daemon stderr lost crash marker",
    )?;
    require(
        failed
            .stderr
            .windows(case.error_type.len())
            .any(|v| v == case.error_type.as_bytes()),
        "daemon stderr lost typed failure",
    )?;
    require(
        fs::read(
            root.join("params")
                .join(&namespace.prefix)
                .join("CarrotException"),
        )? == b"exception",
        "CarrotException raw value differs",
    )?;
    cancel.sleep(Duration::from_millis(200))?;
    require(
        collector.stop(cancel)? == 0,
        "collector SIGTERM did not exit 0",
    )?;
    let mut records = Vec::new();
    for entry in fs::read_dir(&log_root)? {
        let path = entry?.path();
        if path
            .file_name()
            .is_some_and(|s| s.as_bytes().starts_with(b"swaglog."))
        {
            let text = fs::read_to_string(path)?.replace("\r\n", "\n").replace(
                [
                    '\r', '\x0b', '\x0c', '\x1c', '\x1d', '\x1e', '\u{85}', '\u{2028}', '\u{2029}',
                ],
                "\n",
            );
            for line in text.lines() {
                records.push(serde_json::from_str::<Value>(line)?);
            }
        }
    }
    let crashes = records
        .iter()
        .filter(|row| row.get("msg$s") == Some(&json!("crash")))
        .collect::<Vec<_>>();
    require(crashes.len() == 1, "collector must store exactly one crash")?;
    let crash = crashes[0];
    let context = &crash["ctx"];
    require(
        context["daemon"] == case.daemon,
        "crash daemon context differs",
    )?;
    if options.inherited_context {
        require(
            context["dongle_id"] == "owned-device",
            "inherited dongle context differs",
        )?;
        require(
            context["runtime_implementation"] == case.binary,
            "runtime implementation context differs",
        )?;
    }
    require(
        context["runtime_language"] == "rust",
        "runtime language context differs",
    )?;
    let exception = crash["exc_info"]
        .as_str()
        .ok_or("missing typed backtrace")?;
    require(
        exception.contains(case.error_type) && !exception.contains("disabled backtrace"),
        "typed backtrace differs",
    )?;
    let standalone = capture(&command(&binary, &["--help"], &collector_env), cancel)?;
    require(standalone.code == 0, "standalone --help did not exit 0")?;
    let binary_after = executable_digest(&binary)?;
    let collector_after = executable_digest(&collector_binary)?;
    require(
        binary_before.as_deref() == Some(&binary_after)
            && collector_before.as_deref() == Some(&collector_after),
        "managed-runtime input ELF changed",
    )?;
    Ok(
        json!({"binary":case.binary,"sha256":binary_after,"daemon":case.daemon,"typed_failure":case.error_type,"log_records":records.len(),"exit_code":failed.code,"carrot_exception":true,"standalone_help":true,"inherited_context":options.inherited_context}),
    )
}
pub fn run_case(options: &Options, case: &Case) -> Result<Value> {
    let cancel = Cancel::new()?;
    case_inner(options, case, &cancel)
}
pub fn run(options: &Options) -> Result<Value> {
    let receipt_path = options.output.join("result.json");
    match fs::remove_file(&receipt_path) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    let repository = options
        .environment
        .get(OsStr::new("OPENPILOT_RUST_ROOT"))
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let pinned = sources(&repository)?;
    let live = if fs::symlink_metadata(repository.join(SOURCE)).is_ok() {
        pinned.clone()
    } else {
        BTreeMap::new()
    };
    let historical = if live.is_empty() {
        pinned.clone()
    } else {
        BTreeMap::new()
    };
    let cancel = Cancel::new()?;
    let mut rows = Vec::new();
    let collector_path = options.binaries.join("openpilot-logmessaged");
    let before = executable_digest(&collector_path).ok();
    let selected = CASES
        .iter()
        .filter(|c| options.daemon.as_ref().is_none_or(|d| d == c.daemon))
        .collect::<Vec<_>>();
    require(!selected.is_empty(), "no managed-runtime daemon selected")?;
    for case in selected {
        let mut selected = options.clone();
        selected.output = options.output.join(case.daemon);
        rows.push(case_inner(&selected, case, &cancel)?);
    }
    let collector = executable_digest(&collector_path)?;
    require(
        before.as_deref() == Some(&collector),
        "collector changed during checker",
    )?;
    require(
        sources(&repository)? == pinned,
        "source changed during checker",
    )?;
    require(
        fs::symlink_metadata(repository.join(SOURCE)).is_ok() != live.is_empty(),
        "original checker presence changed during execution",
    )?;
    cancel.check()?;
    let receipt = json!({"result":"PASS","collector_sha256":collector,"scenarios":rows,"scope":"owned host daemon entry, real log collector and Params; no hardware","source_sha256":live,"historical_source_sha256":historical,"checker_sha256":digest(Path::new("/proc/self/exe"))?});
    let written = fs::write(
        &receipt_path,
        serde_json::to_string_pretty(&receipt)? + "\n",
    );
    // A signal can arrive while open/write is blocked. Latch its observation
    // once, so cleanup and the returned interruption cannot disagree.
    if let Err(interrupted) = cancel.check() {
        let _ = fs::remove_file(&receipt_path);
        return Err(interrupted);
    }
    written?;
    Ok(receipt)
}
pub fn cli() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let environment = std::env::vars_os().collect();
    match parse(&args, &std::env::current_dir()?, &environment)? {
        Command::Help => println!(
            "usage: check-managed-runtime [-h] --binaries BINARIES --output OUTPUT\n\
             \x20                            [--daemon {{{}}}]\n\
             \x20                            [--omit-inherited-context]\n\n\
             Exercise installed daemon failure boundaries through the actual native\n\
             collector.\n\n\
             options:\n\
             \x20 -h, --help            show this help message and exit\n\
             \x20 --binaries BINARIES\n\
             \x20 --output OUTPUT\n\
             \x20 --daemon {{{}}}\n\
             \x20                       Run one focused daemon boundary\n\
             \x20 --omit-inherited-context\n\
             \x20                       Exercise the managed entry without manager metadata",
            CASES.iter().map(|c| c.daemon).collect::<Vec<_>>().join(","),
            CASES.iter().map(|c| c.daemon).collect::<Vec<_>>().join(",")
        ),
        Command::Run(options) => println!("{}", run(&options)?),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_observed_socket_identity_cannot_be_replaced_before_cleanup() {
        let mut namespace = Namespace::new().unwrap();
        let endpoint = namespace.endpoint.clone();
        let owned = std::os::unix::net::UnixListener::bind(&endpoint).unwrap();
        namespace.observe();
        let original = fs::metadata(&endpoint).unwrap().ino();
        fs::remove_file(&endpoint).unwrap();
        // Collector::stop closes its listener before Namespace is dropped.
        // Keeping the listener alive here hid recycling of its pathname inode.
        drop(owned);
        let mut foreign = None;
        for _ in 0..4096 {
            let replacement = std::os::unix::net::UnixListener::bind(&endpoint).unwrap();
            if fs::metadata(&endpoint).unwrap().ino() == original {
                foreign = Some(replacement);
                break;
            }
            fs::remove_file(&endpoint).unwrap();
            drop(replacement);
        }
        let foreign =
            foreign.unwrap_or_else(|| std::os::unix::net::UnixListener::bind(&endpoint).unwrap());
        let identity = fs::metadata(&endpoint).unwrap().ino();
        namespace.observe();
        drop(namespace);
        let remains = fs::metadata(&endpoint);
        drop(foreign);
        let _ = fs::remove_file(&endpoint);
        assert_eq!(
            remains.unwrap().ino(),
            identity,
            "foreign replacement was unlinked"
        );
    }

    #[test]
    fn observed_owned_endpoint_is_cleaned_after_listener_closes() {
        let mut namespace = Namespace::new().unwrap();
        let endpoint = namespace.endpoint.clone();
        let owned = std::os::unix::net::UnixListener::bind(&endpoint).unwrap();
        namespace.observe();
        drop(owned);
        drop(namespace);
        assert!(!endpoint.exists(), "owned endpoint was not cleaned");
    }
}
