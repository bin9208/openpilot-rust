//! Independent Bluetooth configuration corpus and original validation policy.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    control_tools_policy_check::json,
    path_resolve::resolve_non_strict,
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::{json as value, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, Metadata},
    io::{Read, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
mod cases;
mod reference;
pub use cases::cases;
pub use reference::expected;
const SOURCES: [(&str, &str); 3] = [
    (
        "rust/tools/check_bluetooth_config.py",
        "a8f85f1076773e1e5938e66064e3bb1cd0ea060d7d60df528365108fbcd2d679",
    ),
    (
        "openpilot/selfdrive/carrot/bluetooth/model.py",
        "74f65767da6459358c8809f6e0adf465a4e7007b4bbf4e160b23c98819e6a633",
    ),
    (
        "rust/crates/bluetooth/examples/config_fixture.rs",
        "46d76fbe39859d2706b8c376b5f9f3893a6b6abbcbbb6a7145297488e2cbecc5",
    ),
];
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
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
struct Bound {
    path: PathBuf,
    file: File,
    metadata: Metadata,
    hash: String,
}
impl Bound {
    fn open(path: &Path) -> Result<Self> {
        let fd = rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        let mut file = File::from(fd);
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err("Bluetooth config input is not a regular file".into());
        }
        let mut hasher = Sha256::new();
        let mut chunk = [0; 65536];
        loop {
            let n = file.read(&mut chunk)?;
            if n == 0 {
                break;
            }
            hasher.update(&chunk[..n]);
        }
        let bound = Self {
            path: path.into(),
            file,
            metadata,
            hash: format!("{:x}", hasher.finalize()),
        };
        bound.verify()?;
        Ok(bound)
    }
    fn verify(&self) -> Result<()> {
        if identity(&self.metadata) != identity(&self.file.metadata()?)
            || identity(&self.metadata) != identity(&fs::metadata(&self.path)?)
        {
            return Err(format!("Bluetooth config input changed: {}", self.path.display()).into());
        }
        Ok(())
    }
}
struct Historical {
    path: PathBuf,
    parent: PathBuf,
    directory: File,
    device: u64,
    inode: u64,
}
impl Historical {
    fn open(root: &Path) -> Result<Self> {
        let path = root.join(SOURCES[0].0);
        let parent = path
            .parent()
            .ok_or("config checker parent missing")?
            .to_owned();
        let directory = File::from(rustix::fs::open(
            &parent,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        let metadata = directory.metadata()?;
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) => return Err("Bluetooth config retired checker reappeared".into()),
            Err(error) => return Err(error.into()),
        }
        let historical = Self {
            path,
            parent,
            directory,
            device: metadata.dev(),
            inode: metadata.ino(),
        };
        historical.verify()?;
        Ok(historical)
    }
    fn verify(&self) -> Result<()> {
        let held = self.directory.metadata()?;
        let current = fs::symlink_metadata(&self.parent)?;
        if !held.is_dir()
            || !current.is_dir()
            || (held.dev(), held.ino()) != (self.device, self.inode)
            || (current.dev(), current.ino()) != (self.device, self.inode)
        {
            return Err("Bluetooth config historical checker parent changed".into());
        }
        match fs::symlink_metadata(&self.path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Ok(_) => Err("Bluetooth config retired checker reappeared".into()),
            Err(error) => Err(error.into()),
        }
    }
}
fn sources(root: &Path) -> Result<Vec<Bound>> {
    SOURCES
        .iter()
        .skip(1)
        .map(|(name, expected)| {
            let bound = Bound::open(&root.join(name))?;
            if bound.hash != *expected {
                return Err(format!("Bluetooth config source changed: {name}").into());
            }
            Ok(bound)
        })
        .collect()
}
fn verify(inputs: &[Bound], historical: &Historical, stop: &AtomicBool) -> Result<()> {
    if stop.load(Ordering::SeqCst) {
        return Err("Bluetooth config comparison interrupted".into());
    }
    for input in inputs {
        input.verify()?;
    }
    historical.verify()?;
    Ok(())
}
fn lines(text: &str) -> Vec<&str> {
    let mut lines = text
        .split([
            '\n', '\r', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}',
            '\u{2029}',
        ])
        .collect::<Vec<_>>();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines
}
fn object_keys(value: &JsonValue) -> Result<Vec<Vec<u32>>> {
    match value.view() {
        JsonView::Object(items) => Ok(items.into_iter().map(|(key, _)| key.to_vec()).collect()),
        _ => Err("Bluetooth config candidate map is not an object".into()),
    }
}
fn ordered_maps(source: &JsonValue, candidate: &JsonValue) -> Result<()> {
    let Some(devices) = source.get("devices") else {
        return Ok(());
    };
    let other = candidate
        .get("devices")
        .ok_or("Bluetooth config candidate devices absent")?;
    if object_keys(&devices)? != object_keys(&other)? {
        return Err("Bluetooth config device order differs".into());
    }
    let JsonView::Object(entries) = devices.view() else {
        unreachable!()
    };
    let JsonView::Object(actual) = other.view() else {
        unreachable!()
    };
    for ((_, device), (_, candidate)) in entries.into_iter().zip(actual) {
        if object_keys(&device.get("mapping").ok_or("source mapping absent")?)?
            != object_keys(&candidate.get("mapping").ok_or("candidate mapping absent")?)?
        {
            return Err("Bluetooth config mapping order differs".into());
        }
    }
    Ok(())
}
pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub output: PathBuf,
}
struct PendingReceipt {
    path: PathBuf,
    file: tempfile::NamedTempFile,
    inputs: Vec<Bound>,
    historical: Historical,
}
impl PendingReceipt {
    fn commit(self, stop: &AtomicBool) -> Result<()> {
        verify(&self.inputs, &self.historical, stop)?;
        let held = self
            .file
            .persist_noclobber(&self.path)
            .map_err(|e| e.error)?;
        let result = verify(&self.inputs, &self.historical, stop);
        if result.is_err()
            && held.metadata().is_ok_and(|m| {
                fs::symlink_metadata(&self.path)
                    .is_ok_and(|n| (m.dev(), m.ino()) == (n.dev(), n.ino()))
            })
        {
            let _ = fs::remove_file(&self.path);
        }
        result
    }
}
pub fn check(
    options: &Options,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
    stop: &AtomicBool,
) -> Result<Value> {
    let (report, pending) = run_inner(options, runner, stop)?;
    finish(report, pending, printed, stop)
}
fn finish(
    report: Value,
    pending: PendingReceipt,
    printed: &mut dyn Write,
    stop: &AtomicBool,
) -> Result<Value> {
    verify(&pending.inputs, &pending.historical, stop)?;
    writeln!(printed, "{}", serde_json::to_string(&report)?)?;
    printed.flush()?;
    pending.commit(stop)?;
    Ok(report)
}
fn run_inner(
    options: &Options,
    runner: &mut dyn Runner,
    stop: &AtomicBool,
) -> Result<(Value, PendingReceipt)> {
    let historical = Historical::open(&options.root)?;
    let mut bound = sources(&options.root)?;
    verify(&bound, &historical, stop)?;
    if let Some(parent) = options
        .output
        .parent()
        .filter(|v| !v.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&options.output)?;
    let rows = cases()?;
    let mut input = String::new();
    let mut reference = String::new();
    let mut expected = Vec::with_capacity(rows.len());
    for row in &rows {
        input.push_str(&row.to_json()?);
        input.push('\n');
        let answer = reference::expected(row)?;
        reference.push_str(&answer.to_json()?);
        reference.push('\n');
        expected.push(answer);
    }
    fs::write(options.output.join("input.jsonl"), &input)?;
    fs::write(options.output.join("original.jsonl"), &reference)?;
    let binary = resolve_non_strict(&options.binary)?;
    let argument = if options.binary.is_absolute() {
        options.binary.clone()
    } else {
        std::env::current_dir()?.join(&options.binary)
    };
    let artifact = Bound::open(&binary)?;
    let alias = Bound::open(&argument)?;
    let mut command =
        CommandSpec::new_os(vec![binary.as_os_str().to_owned()], Duration::from_secs(30));
    command.input = Some(input.into_bytes());
    command.stdout_limit = usize::MAX;
    command.stderr_limit = usize::MAX;
    verify(&bound, &historical, stop)?;
    let Captured {
        code,
        stdout,
        stderr,
    } = runner.run(&command)?;
    let stdout = String::from_utf8(stdout)?;
    let stderr = String::from_utf8(stderr)?;
    fs::write(options.output.join("native.jsonl"), &stdout)?;
    fs::write(options.output.join("native.stderr"), stderr)?;
    if code != 0 {
        return Err(format!("Bluetooth config fixture exited {code}").into());
    }
    let normalized = stdout.replace("\r\n", "\n").replace('\r', "\n");
    let lines = lines(&normalized);
    if lines.len() != expected.len() {
        return Err("Bluetooth config result cardinality differs".into());
    }
    for (index, (line, source)) in lines.into_iter().zip(&expected).enumerate() {
        let native = json::parse_text(line)?;
        if !json::equal(source, &native) {
            return Err(format!("Bluetooth config response differs: {index}").into());
        }
        ordered_maps(source, &native)?;
    }
    artifact.verify()?;
    alias.verify()?;
    let binary_sha = artifact.hash.clone();
    bound.push(artifact);
    bound.push(alias);
    verify(&bound, &historical, stop)?;
    let report = value!({"cases": rows.len(), "accepted": expected.iter().filter(|v| v.get("error").is_none()).count(), "source_sha256": SOURCES[1].1, "binary_sha256": binary_sha,
        "input_sha256": digest(&fs::read(options.output.join("input.jsonl"))?), "original_sha256": digest(reference.as_bytes())});
    verify(&bound, &historical, stop)?;
    let receipt = options.output.join("result.json");
    let mut pending = tempfile::NamedTempFile::new_in(&options.output)?;
    pending.write_all(serde_json::to_string_pretty(&report)?.as_bytes())?;
    verify(&bound, &historical, stop)?;
    Ok((
        report,
        PendingReceipt {
            path: receipt,
            file: pending,
            inputs: bound,
            historical,
        },
    ))
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
    let args = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if args.help {
        println!("usage: check-bluetooth-config --binary BINARY --output OUTPUT");
        return Ok(());
    }
    let stop = Arc::new(AtomicBool::new(false));
    let mut signals = Signals(vec![]);
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signals
            .0
            .push(signal_hook::flag::register(signal, stop.clone())?);
    }
    let options = Options {
        root: std::env::var_os("OPENPILOT_RUST_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")),
        binary: args.values["--binary"].clone().into(),
        output: args.values["--output"].clone().into(),
    };
    let result = run_inner(&options, &mut SignalRunner(stop.clone()), &stop);
    // Keep the pending receipt private until the handlers can no longer latch
    // a late cancellation, then publish using the still-held input identities.
    drop(signals);
    if stop.load(Ordering::SeqCst) {
        return Err("Bluetooth config comparison interrupted".into());
    }
    let (report, pending) = result?;
    finish(report, pending, &mut std::io::stdout(), &stop)?;
    Ok(())
}
