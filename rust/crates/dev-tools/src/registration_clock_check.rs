//! Independent CPython UTC calendar policy and actual SystemTime probe controller.
//! Fixtures and the candidate registration crate never supply expected values.
use crate::{
    argparse_scan::ArgumentError,
    control_tools_policy_check::json,
    path_resolve::resolve_non_strict,
    python_fs,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::JsonValue;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fmt, fs,
    io::{self, Read, Write},
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub const SOURCE: &str = "rust/tools/check_registration_clock.py";
pub const SOURCE_SHA: &str = "4641d2e73a12c476a67b418210148a54e178129b94abf65b968a107f302b1920";
pub const USAGE: &str = "usage: check-registration-clock [-h] [--runner RUNNER] binary output";
pub const HELP: &str = "usage: check-registration-clock [-h] [--runner RUNNER] binary output\n\npositional arguments:\n  binary\n  output\n\noptions:\n  -h, --help       show this help message and exit\n  --runner RUNNER\n";
pub const SUCCESS: &str =
    "13 real SystemTime/Python UTC floor, calendar and one-hour expiry boundary cases PASS\n";
const CASES: [(i64, u32); 13] = [
    (0, 0),
    (0, 999_999_999),
    (-1, 999_999_999),
    (-1, 0),
    (-2, 999_999_999),
    (-62_135_596_800, 0),
    (-62_135_596_801, 999_999_999),
    (253_402_297_199, 999_999_999),
    (253_402_297_200, 0),
    (253_402_300_799, 999_999_999),
    (253_402_300_800, 0),
    (i64::MAX, 0),
    (i64::MIN, 0),
];
pub fn cases() -> &'static [(i64, u32)] {
    &CASES
}
const fn days_before_year(year: i64) -> i64 {
    let y = year - 1;
    365 * y + y / 4 - y / 100 + y / 400
}
/// `fromtimestamp(integer, UTC).replace(microsecond=nanos//1000)` followed by
/// `calendar.timegm` and a one-hour naive datetime addition. Gregorian year
/// limits are derived here rather than imported from the candidate runtime.
pub fn source_value(seconds: i64, nanos: u32) -> serde_json::Value {
    const FIRST: i64 = -days_before_year(1970) * 86_400;
    const END: i64 = (days_before_year(10_000) - days_before_year(1970)) * 86_400;
    if !(FIRST..END).contains(&seconds) || nanos / 1000 > 999_999 {
        return serde_json::json!({"seconds":null,"expiration":null});
    }
    let expiration = seconds.checked_add(3600).filter(|s| *s < END);
    serde_json::json!({"seconds":seconds,"expiration":expiration})
}
#[derive(Debug, Clone)]
pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub output: PathBuf,
    pub runner: Vec<OsString>,
    pub environment: BTreeMap<OsString, OsString>,
    /// Native owned-child bound; the old Python subprocess was unbounded.
    pub timeout: Duration,
}
#[derive(Debug)]
pub enum Command {
    Help,
    Run(Options),
}
#[derive(Debug, PartialEq, Eq)]
pub struct SourceProof {
    pub live: BTreeMap<String, String>,
    pub historical: BTreeMap<String, String>,
}
/// Only genuine absence of the retired checker is admitted. In particular a
/// dangling symlink is a present object, and cannot bypass source drift checks.
pub fn sources(root: &Path) -> Result<SourceProof> {
    let mut proof = SourceProof {
        live: BTreeMap::new(),
        historical: BTreeMap::new(),
    };
    match fs::symlink_metadata(root.join(SOURCE)) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            proof.historical.insert(SOURCE.into(), SOURCE_SHA.into());
        }
        Err(e) => return Err(e.into()),
        Ok(_) => {
            if Artifact::open(&root.join(SOURCE))?.sha256 != SOURCE_SHA {
                return Err("registration clock original source changed".into());
            }
            proof.live.insert(SOURCE.into(), SOURCE_SHA.into());
        }
    }
    Ok(proof)
}
fn argument(message: impl fmt::Display) -> ArgumentError {
    ArgumentError(format!(
        "{USAGE}\ncheck-registration-clock: error: {message}"
    ))
}
fn display(value: &OsStr) -> String {
    python_fs::codepoints(value)
        .into_iter()
        .map(|p| char::from_u32(p).map_or_else(|| format!("\\u{p:04x}"), |c| c.to_string()))
        .collect()
}
fn option(value: &OsStr) -> bool {
    let bytes = value.as_bytes();
    if !bytes.starts_with(b"-") || bytes == b"-" || bytes.contains(&b' ') {
        return false;
    }
    let text = display(value);
    let body = text
        .strip_prefix('-')
        .unwrap_or("")
        .strip_suffix('\n')
        .unwrap_or(&text[1..]);
    let decimal = |s: &str| {
        s.chars()
            .all(|c| crate::argparse_scan::decimal_value(c).is_some())
    };
    let negative = if let Some((a, b)) = body.split_once('.') {
        !b.is_empty() && decimal(a) && decimal(b)
    } else {
        !body.is_empty() && decimal(body)
    };
    !negative
}
/// Preserve repeated runner values, long abbreviations, help action ordering,
/// raw OS positionals and argparse's required-positionals-before-extras phase.
pub fn parse(
    args: &[OsString],
    cwd: &Path,
    environment: BTreeMap<OsString, OsString>,
) -> Result<Command> {
    let mut positions = Vec::new();
    let mut runners = Vec::new();
    let mut extras = Vec::new();
    let mut literal = false;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let bytes = arg.as_bytes();
        if !literal && bytes == b"--" {
            literal = true;
            i += 1;
            continue;
        }
        let name = bytes.split(|b| *b == b'=').next().unwrap_or(bytes);
        let help = name.starts_with(b"-h") && !name.starts_with(b"--")
            || name.len() > 2 && b"--help".starts_with(name);
        let runner = name.len() > 2 && b"--runner".starts_with(name);
        // argparse resolves known option names before its unknown-token rule
        // that treats spaces as positional text. Inline values can have spaces.
        if !literal && (help || runner || option(arg)) {
            let inline = bytes
                .iter()
                .position(|b| *b == b'=')
                .map(|p| &bytes[p + 1..]);
            if help {
                if let Some(value) = inline {
                    let points = python_fs::codepoints(OsStr::from_bytes(value));
                    return Err(argument(format!(
                        "argument -h/--help: ignored explicit argument {}",
                        crate::plot_samples_source::text_repr(&points)
                    ))
                    .into());
                }
                return Ok(Command::Help);
            }
            if runner {
                let value = if let Some(v) = inline {
                    OsStr::from_bytes(v).to_owned()
                } else {
                    i += 1;
                    args.get(i)
                        .filter(|v| !option(v))
                        .cloned()
                        .ok_or_else(|| argument("argument --runner: expected one argument"))?
                };
                runners.push(value);
            } else {
                extras.push(arg.clone());
            }
        } else if positions.len() < 2 {
            positions.push(arg.clone());
        } else {
            extras.push(arg.clone());
        }
        i += 1;
    }
    if positions.len() < 2 {
        return Err(argument(if positions.is_empty() {
            "the following arguments are required: binary, output"
        } else {
            "the following arguments are required: output"
        })
        .into());
    }
    if !extras.is_empty() {
        return Err(argument(format!(
            "unrecognized arguments: {}",
            extras
                .iter()
                .map(|v| display(v))
                .collect::<Vec<_>>()
                .join(" ")
        ))
        .into());
    }
    let path = |v: &OsStr| {
        if Path::new(v).is_absolute() {
            PathBuf::from(v)
        } else {
            cwd.join(v)
        }
    };
    let root = environment
        .get(OsStr::new("OPENPILOT_RUST_ROOT"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    Ok(Command::Run(Options {
        root,
        binary: path(&positions[0]),
        output: path(&positions[1]),
        runner: runners,
        environment,
        timeout: Duration::from_secs(60),
    }))
}
#[derive(Debug)]
pub struct ChildFailure {
    pub code: i32,
    pub stderr: String,
}
impl fmt::Display for ChildFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "registration clock child exited {}: {}",
            self.code, self.stderr
        )
    }
}
impl std::error::Error for ChildFailure {}
fn interrupted(cancelled: &AtomicBool) -> Result<()> {
    if cancelled.load(Ordering::Relaxed) {
        Err(io::Error::from(io::ErrorKind::Interrupted).into())
    } else {
        Ok(())
    }
}
fn command(options: &Options) -> Result<(Vec<OsString>, CommandSpec)> {
    let mut argv = options.runner.clone();
    argv.push(resolve_non_strict(&options.binary)?.into_os_string());
    let mut spec = CommandSpec::new_os(argv.clone(), options.timeout);
    spec.input = Some(
        CASES
            .iter()
            .map(|(s, n)| format!("[{s}, {n}]\n"))
            .collect::<String>()
            .into_bytes(),
    );
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    spec.set_os_env(options.environment.clone());
    spec.env_remove = std::env::vars_os()
        .map(|(k, _)| k)
        .filter(|k| !options.environment.contains_key(k))
        .collect();
    Ok((argv, spec))
}
fn lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines = text
        .split([
            '\n', '\x0b', '\x0c', '\x1c', '\x1d', '\x1e', '\u{85}', '\u{2028}', '\u{2029}',
        ])
        .collect::<Vec<_>>();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines
}
fn regular_file(path: &Path) -> Result<fs::File> {
    use rustix::fs::{open, Mode, OFlags};
    // A FIFO must not block an eager fingerprint before the original spawn
    // error. Open nonblocking and inspect this FD, not a racy path metadata.
    let file = fs::File::from(open(
        path,
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?);
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected regular registration clock input",
        )
        .into());
    }
    Ok(file)
}
struct Artifact {
    // Retain the original inode while the actual child runs.
    _file: fs::File,
    device: u64,
    inode: u64,
    sha256: String,
}
impl Artifact {
    fn open(path: &Path) -> Result<Self> {
        let mut file = regular_file(path)?;
        let metadata = file.metadata()?;
        let mut digest = Sha256::new();
        let mut buffer = [0; 65536];
        loop {
            let size = file.read(&mut buffer)?;
            if size == 0 {
                break;
            }
            digest.update(&buffer[..size]);
        }
        Ok(Self {
            _file: file,
            device: metadata.dev(),
            inode: metadata.ino(),
            sha256: format!("{digest:x}", digest = digest.finalize()),
        })
    }
    fn verify(&self, path: &Path) -> Result<()> {
        let current = Self::open(path)?;
        if (self.device, self.inode, &self.sha256)
            != (current.device, current.inode, &current.sha256)
        {
            return Err("registration clock input changed during execution".into());
        }
        Ok(())
    }
}
fn invalidate_stale_pass(path: &Path) -> Result<()> {
    let Ok(identity) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    if !identity.is_file() && !identity.file_type().is_symlink() {
        return Ok(());
    }
    // Inspection failures and arbitrary failure artifacts keep the original
    // eventual write phase. Only a readable, regular equal:true report is stale
    // success owned by this output argument. Never block reading a FIFO here.
    let Ok(mut file) = regular_file(path) else {
        return Ok(());
    };
    if file.metadata()?.len() > 16 * 1024 * 1024 {
        return Ok(());
    }
    let mut bytes = Vec::new();
    if (&mut file)
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return Ok(());
    }
    let Ok(value) = json::parse(&bytes) else {
        return Ok(());
    };
    if !value
        .get("equal")
        .is_some_and(|v| matches!(v.view(), openpilot_log_format::JsonView::Bool(true)))
    {
        return Ok(());
    }
    if fs::symlink_metadata(path)
        .is_ok_and(|m| m.dev() == identity.dev() && m.ino() == identity.ino())
    {
        fs::remove_file(path)?;
    } else {
        return Err("registration clock output changed during stale PASS invalidation".into());
    }
    Ok(())
}
fn verify_sources(root: &Path, before: &SourceProof) -> Result<()> {
    if &sources(root)? != before {
        return Err("registration clock source presence changed during execution".into());
    }
    Ok(())
}
fn execute(
    options: &Options,
    runner: &mut dyn Runner,
    cancelled: &AtomicBool,
) -> Result<JsonValue> {
    invalidate_stale_pass(&options.output)?;
    let source_before = sources(&options.root)?;
    interrupted(cancelled)?;
    let (argv, spec) = command(options)?;
    // Preserve child/decoder failure order: an unavailable or nonregular
    // fingerprint becomes an error only at the original post-parse hash phase.
    let artifact = Artifact::open(&options.binary);
    let child = runner.run(&spec)?;
    // Python text=True decodes both pipes before applying check=True.
    let stdout = String::from_utf8(child.stdout)?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let stderr = String::from_utf8(child.stderr)?;
    if child.code != 0 {
        return Err(ChildFailure {
            code: child.code,
            stderr,
        }
        .into());
    }
    let actual = lines(&stdout)
        .into_iter()
        .map(json::parse_text)
        .collect::<Result<Vec<_>>>()?;
    let actual = JsonValue::parse(&format!(
        "[{}]",
        actual
            .iter()
            .map(JsonValue::to_json)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join(", ")
    ))?;
    let expected = JsonValue::parse(&format!(
        "[{}]",
        CASES
            .iter()
            .map(|&(s, n)| {
                let v = source_value(s, n);
                format!(
                    "{{\"seconds\":{},\"expiration\":{}}}",
                    v["seconds"], v["expiration"]
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    ))?;
    let equal = json::equal(&expected, &actual);
    let argv = argv
        .iter()
        .map(|s| python_fs::json_string(s))
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    let artifact = artifact?;
    artifact.verify(&options.binary)?;
    verify_sources(&options.root, &source_before)?;
    let report=JsonValue::parse(&format!("{{\"argv\":[{argv}],\"cases\":{},\"source\":{},\"native\":{},\"exit\":{},\"binary_sha256\":\"{}\",\"equal\":{equal}}}",serde_json::to_string(&CASES)?,expected.to_json()?,actual.to_json()?,child.code,artifact.sha256))?;
    let text = json::pretty(&report, 0)?;
    interrupted(cancelled)?;
    // Do not create the parent or append a newline: match Path.write_text.
    let mut output = fs::File::create(&options.output)?;
    let identity = output.metadata()?;
    output.write_all(text.as_bytes())?;
    let publication = interrupted(cancelled)
        .and_then(|()| artifact.verify(&options.binary))
        .and_then(|()| verify_sources(&options.root, &source_before));
    if let Err(error) = publication {
        // This invocation created the report; no stale PASS after a signal
        // delivered while a blocking output open/write was in progress.
        if fs::symlink_metadata(&options.output)
            .is_ok_and(|m| m.dev() == identity.dev() && m.ino() == identity.ino())
        {
            fs::remove_file(&options.output)?;
        }
        return Err(error);
    }
    if !equal {
        return Err("registration clock original/native results differ (report retained)".into());
    }
    Ok(report)
}
pub fn run_with(options: &Options, runner: &mut dyn Runner) -> Result<JsonValue> {
    execute(options, runner, &AtomicBool::new(false))
}
struct Controlled<'a>(&'a AtomicBool);
impl Runner for Controlled<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<crate::qa_process::Captured> {
        NativeRunner.run_controlled(spec, Instant::now() + spec.timeout, self.0)
    }
}
pub fn run_controlled(options: &Options, cancelled: &AtomicBool) -> Result<JsonValue> {
    execute(options, &mut Controlled(cancelled), cancelled)
}
struct Signals {
    cancelled: Arc<AtomicBool>,
    handlers: Vec<signal_hook::SigId>,
}
impl Signals {
    fn new() -> Result<Self> {
        let mut this = Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            handlers: Vec::new(),
        };
        for s in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            this.handlers
                .push(signal_hook::flag::register(s, Arc::clone(&this.cancelled))?);
        }
        Ok(this)
    }
}
impl Drop for Signals {
    fn drop(&mut self) {
        for h in self.handlers.drain(..) {
            signal_hook::low_level::unregister(h);
        }
    }
}
pub fn cli() -> Result<()> {
    match parse(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &std::env::current_dir()?,
        std::env::vars_os().collect(),
    )? {
        Command::Help => print!("{HELP}"),
        Command::Run(options) => {
            let signals = Signals::new()?;
            run_controlled(&options, &signals.cancelled)?;
            print!("{SUCCESS}");
        }
    }
    Ok(())
}
