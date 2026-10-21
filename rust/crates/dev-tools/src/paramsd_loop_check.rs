//! Native paramsd loop controller. The expected rows come only from the sealed
//! original observation in `paramsd_loop_reference`.
use crate::{
    paramsd_loop_fixture_generate,
    paramsd_loop_reference::{self, Summary},
    path_resolve::resolve_non_strict,
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, Metadata},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const SOURCES: [(&str, Option<&str>); 6] = [
    (
        "rust/tools/paramsd_loop_fixture.py",
        Some("767f58edf7163edf8eaa493b5453f3bc00f6966772dba932e96a294e3efe3089"),
    ),
    (
        "rust/tools/paramsd_fixture.py",
        Some("8c0356bb780039c475def68358567489e036ab422921b8389bc7fbed20635521"),
    ),
    (
        "rust/tools/check_paramsd_loop.py",
        Some("56c9afe7bc9bb21220478f21405ebf5327a931b0403c7cf628203e0b49fd58c3"),
    ),
    (
        "rust/crates/dev-tools/src/paramsd_loop_fixture_generate.rs",
        None,
    ),
    ("rust/crates/dev-tools/src/paramsd_loop_reference.rs", None),
    ("rust/crates/paramsd/examples/paramsd_loop.rs", None),
];
const RAW_LIMIT: u64 = 32_000_000;

pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub evidence: PathBuf,
}

fn identity(metadata: &Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
    (
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        metadata.mtime(),
        metadata.mtime_nsec(),
        metadata.ctime(),
        metadata.ctime_nsec(),
    )
}

struct Bound {
    path: PathBuf,
    file: File,
    metadata: Metadata,
    hash: String,
    limit: Option<u64>,
}
impl Bound {
    fn open(path: PathBuf, expected: Option<&str>) -> Result<Self> {
        Self::open_with_limit(path, expected, None)
    }
    fn open_with_limit(path: PathBuf, expected: Option<&str>, limit: Option<u64>) -> Result<Self> {
        let fd = rustix::fs::open(
            &path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::CLOEXEC
                | rustix::fs::OFlags::NOFOLLOW,
            rustix::fs::Mode::empty(),
        )?;
        let mut file = File::from(fd);
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(format!(
                "paramsd loop input is not a regular file: {}",
                path.display()
            )
            .into());
        }
        let hash = Self::hash_file(&mut file, &metadata, limit)?;
        if expected.is_some_and(|expected| expected != hash) {
            return Err(format!("paramsd loop source hash changed: {}", path.display()).into());
        }
        let bound = Self {
            path,
            file,
            metadata,
            hash,
            limit,
        };
        bound.verify()?;
        Ok(bound)
    }
    fn verify(&self) -> Result<()> {
        let current = Self::hash_path(&self.path, self.limit)?;
        if identity(&self.metadata) != identity(&self.file.metadata()?)
            || identity(&self.metadata) != identity(&current.0)
            || self.hash != current.1
        {
            return Err(format!("paramsd loop input changed: {}", self.path.display()).into());
        }
        Ok(())
    }
    fn hash_file(file: &mut File, metadata: &Metadata, limit: Option<u64>) -> Result<String> {
        if limit.is_some_and(|max| metadata.len() > max) {
            return Err("paramsd loop raw output exceeds 32,000,000 bytes".into());
        }
        let mut hasher = Sha256::new();
        let mut chunk = [0; 65536];
        let mut total = 0_u64;
        loop {
            let available = limit.map_or(chunk.len(), |max| {
                (max.saturating_sub(total) + 1).min(chunk.len() as u64) as usize
            });
            let count = file.read(&mut chunk[..available])?;
            if count == 0 {
                break;
            }
            total = total
                .checked_add(u64::try_from(count)?)
                .ok_or("paramsd loop input length overflow")?;
            if limit.is_some_and(|max| total > max) {
                return Err("paramsd loop raw output exceeds 32,000,000 bytes".into());
            }
            hasher.update(&chunk[..count]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }
    fn hash_path(path: &Path, limit: Option<u64>) -> Result<(Metadata, String)> {
        let fd = rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::CLOEXEC
                | rustix::fs::OFlags::NOFOLLOW,
            rustix::fs::Mode::empty(),
        )?;
        let mut file = File::from(fd);
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err("paramsd loop input replaced by nonregular file".into());
        }
        let hash = Self::hash_file(&mut file, &metadata, limit)?;
        if identity(&metadata) != identity(&file.metadata()?) {
            return Err("paramsd loop input changed while hashing".into());
        }
        Ok((metadata, hash))
    }
}

fn native_elf(bound: &mut Bound) -> Result<()> {
    let machine: u16 = match std::env::consts::ARCH {
        "x86_64" => 62,
        "aarch64" => 183,
        _ => return Err("unsupported host architecture for paramsd loop candidate".into()),
    };
    bound.file.seek(SeekFrom::Start(0))?;
    let mut header = [0_u8; 64];
    if bound.file.read_exact(&mut header).is_err() {
        return Err("paramsd loop candidate is not a native host ELF".into());
    }
    if &header[..4] != b"\x7fELF"
        || header[4] != 2 // ELFCLASS64
        || header[5] != 1 // little endian
        || header[6] != 1 // current ELF version
        || !matches!(u16::from_le_bytes([header[16], header[17]]), 2 | 3)
        || u16::from_le_bytes([header[18], header[19]]) != machine
    {
        return Err("paramsd loop candidate is not a native host ELF".into());
    }
    bound.verify()
}

fn check(stop: &AtomicBool, bounds: &[Bound]) -> Result<()> {
    if stop.load(Ordering::SeqCst) {
        return Err("paramsd loop comparison interrupted".into());
    }
    for bound in bounds {
        bound.verify()?;
    }
    Ok(())
}

fn path_value(path: &Path) -> Value {
    path.to_str().map_or_else(
        || json!({"Unix":path.as_os_str().as_bytes()}),
        |text| json!(text),
    )
}

fn bindings(bounds: &[Bound]) -> Value {
    Value::Array(
        bounds
            .iter()
            .map(|bound| json!({"path":path_value(&bound.path),"sha256":bound.hash}))
            .collect(),
    )
}

fn report(summary: &Summary, bounds: &[Bound]) -> Value {
    json!({"result":"pass","cases":summary.cases,"frames":summary.frames,
        "saves":summary.saves,"gps_writes":summary.gps_writes,
        "sources":bindings(bounds)})
}

pub struct Completed {
    report: Value,
    evidence: PathBuf,
    bounds: Vec<Bound>,
}

pub fn prepare(options: &Options, runner: &mut dyn Runner, stop: &AtomicBool) -> Result<Completed> {
    if let Some(parent) = options
        .evidence
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&options.evidence)?; // Existing evidence is never a success input or overwrite target.
    let evidence = resolve_non_strict(&options.evidence)?;
    let root = resolve_non_strict(&options.root)?;
    let mut bounds = Vec::new();
    for (name, expected) in SOURCES {
        bounds.push(Bound::open(root.join(name), expected)?);
    }
    let binary = resolve_non_strict(&options.binary)?;
    let mut candidate = Bound::open(binary.clone(), None)?;
    native_elf(&mut candidate)?;
    bounds.push(candidate);
    check(stop, &bounds)?;
    let request = paramsd_loop_fixture_generate::cases()?;
    paramsd_loop_reference::verify_input(&request)?;
    if request["cases"]
        .as_array()
        .is_none_or(|rows| rows.len() != 39)
    {
        return Err("paramsd loop input case count differs".into());
    }
    let request = serde_json::to_vec(&request)?;
    let input = evidence.join("input.json");
    fs::write(&input, &request)?;
    bounds.push(Bound::open(
        input,
        Some(&format!("{:x}", Sha256::digest(&request))),
    )?);
    let raw = evidence.join("native-raw.json");
    let mut command = CommandSpec::new_os(
        vec![binary.clone().into_os_string(), raw.as_os_str().to_owned()],
        Duration::from_secs(180),
    );
    command.input = Some(request);
    command.stdout_limit = 16 * 1024 * 1024;
    command.stderr_limit = 16 * 1024 * 1024;
    fs::write(
        evidence.join("invocation.json"),
        serde_json::to_vec_pretty(
            &json!({"argv":[path_value(&binary),path_value(&raw)],"timeout_seconds":180,
                "binary_sha256":bounds[SOURCES.len()].hash,"input_sha256":bounds[SOURCES.len()+1].hash}),
        )?,
    )?;
    check(stop, &bounds)?;
    fs::write(
        evidence.join("sources-before.json"),
        serde_json::to_vec_pretty(&bindings(&bounds))?,
    )?;
    let captured = runner.run_to_files(
        &command,
        &evidence.join("stdout.log"),
        &evidence.join("stderr.log"),
    );
    let Captured { code, .. } = match captured {
        Ok(value) => value,
        Err(error) => {
            fs::write(
                evidence.join("process.json"),
                serde_json::to_vec_pretty(&json!({"error":error.to_string()}))?,
            )?;
            return Err(error);
        }
    };
    fs::write(
        evidence.join("process.json"),
        serde_json::to_vec_pretty(&json!({"exit_code":code}))?,
    )?;
    check(stop, &bounds)?;
    if code != 0 {
        return Err(format!("paramsd loop candidate exited {code}").into());
    }
    let output = Bound::open_with_limit(raw.clone(), None, Some(RAW_LIMIT))?;
    let summary = paramsd_loop_reference::compare_file(&raw)?;
    bounds.push(output);
    check(stop, &bounds)?;
    fs::write(
        evidence.join("sources-after.json"),
        serde_json::to_vec_pretty(&bindings(&bounds))?,
    )?;
    Ok(Completed {
        report: report(&summary, &bounds),
        evidence,
        bounds,
    })
}

pub fn finish(done: Completed, printed: &mut dyn Write, stop: &AtomicBool) -> Result<Value> {
    check(stop, &done.bounds)?;
    writeln!(printed, "{}", serde_json::to_string(&done.report)?)?;
    printed.flush()?;
    check(stop, &done.bounds)?;
    let path = done.evidence.join("result.json");
    let mut pending = tempfile::NamedTempFile::new_in(&done.evidence)?;
    pending.write_all(serde_json::to_string_pretty(&done.report)?.as_bytes())?;
    check(stop, &done.bounds)?;
    let held = pending
        .persist_noclobber(&path)
        .map_err(|error| error.error)?;
    if let Err(error) = check(stop, &done.bounds) {
        if let (Ok(own), Ok(current)) = (held.metadata(), fs::symlink_metadata(&path)) {
            if (own.dev(), own.ino()) == (current.dev(), current.ino()) {
                let _ = fs::remove_file(&path);
            }
        }
        return Err(error);
    }
    Ok(done.report)
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
    let mut binary = None;
    let mut evidence = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(option) = args.next() {
        let slot = match option.to_str() {
            Some("--binary") => &mut binary,
            Some("--evidence") => &mut evidence,
            Some("--help") if binary.is_none() && evidence.is_none() => {
                println!("usage: check-paramsd-loop-native --binary BINARY --evidence DIRECTORY");
                return Ok(());
            }
            _ => return Err("unknown or duplicate paramsd loop option".into()),
        };
        if slot.is_some() {
            return Err("duplicate paramsd loop option".into());
        }
        *slot = Some(PathBuf::from(
            args.next().ok_or("paramsd loop option value missing")?,
        ));
    }
    let options = Options {
        root: std::env::var_os("OPENPILOT_RUST_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")),
        binary: binary.ok_or("--binary required")?,
        evidence: evidence.ok_or("--evidence required")?,
    };
    let stop = Arc::new(AtomicBool::new(false));
    let mut signals = Signals(Vec::new());
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signals
            .0
            .push(signal_hook::flag::register(signal, stop.clone())?);
    }
    let prepared = prepare(&options, &mut SignalRunner(stop.clone()), &stop)?;
    let result = finish(prepared, &mut std::io::stdout(), &stop);
    drop(signals);
    result?;
    Ok(())
}
