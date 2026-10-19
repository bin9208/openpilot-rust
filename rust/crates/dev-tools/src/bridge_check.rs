//! Native controller for the unchanged source/native bridge lifecycle matrix.
//! The original Python controller remains a mandatory independent CI control.
use crate::{
    argparse_scan::{scan_os, ArgumentError, OptionSpec},
    path_resolve::resolve_non_strict,
    qa_process::{CommandSpec, NativeRunner},
    seal::Result,
};
use rustix::process::{kill_process, Pid, Signal};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
mod identity;
mod publication;
mod traffic;
const SOURCE_SHA: &str = "17d5fc9f1d5dc3c59306ba0a8fdcc3bd85b216b362fbbe18fdc7e5daaa246821";
const PASS: &str = "PASS: source/native bridge bidirectional bursts, substring whitelist, client disconnect/reconnect and shutdown";
pub const SCENARIOS: [&str; 2] = ["outgoing", "incoming"];
#[derive(Debug)]
pub struct Options {
    pub original: Option<PathBuf>,
    pub binary: PathBuf,
    pub output: PathBuf,
    pub scenario: Option<String>,
}
pub fn parse(args: &[OsString]) -> Result<Option<Options>> {
    let parsed = scan_os(
        args,
        &[
            OptionSpec::value("--original", false),
            OptionSpec::value("--binary", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--scenario", false),
        ],
    )?;
    if parsed.help {
        return Ok(None);
    }
    let scenario = parsed
        .values
        .get("--scenario")
        .map(|value| {
            value
                .to_str()
                .filter(|v| SCENARIOS.contains(v))
                .map(str::to_owned)
                .ok_or_else(|| ArgumentError("--scenario: expected outgoing or incoming".into()))
        })
        .transpose()?;
    let original = parsed
        .values
        .get("--original")
        .map(|v| resolve_non_strict(Path::new(v)))
        .transpose()?;
    if scenario.is_none() && original.is_none() {
        return Err(Box::new(ArgumentError(
            "--original is required for source/native comparison".into(),
        )));
    }
    Ok(Some(Options {
        original,
        scenario,
        binary: resolve_non_strict(Path::new(&parsed.values["--binary"]))?,
        output: resolve_non_strict(Path::new(&parsed.values["--output"]))?,
    }))
}
pub fn port(name: &str) -> u16 {
    let mut value = 0xcbf29ce484222325u64;
    for byte in name.bytes() {
        value = (value ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    (8023 + value % (65535 - 8023)) as u16
}
pub fn packets(scenario: &str) -> Result<Vec<Vec<u8>>> {
    let suffix: &[u8] = match scenario {
        "outgoing" => b"\x00\xffbridge",
        "incoming" => b"incoming\x00\xff",
        _ => return Err("unknown bridge scenario".into()),
    };
    Ok((0..160u32)
        .map(|n| [n.to_le_bytes().as_slice(), suffix].concat())
        .collect())
}
pub fn packet_digest(scenario: &str) -> Result<String> {
    let mut hash = Sha256::new();
    for packet in packets(scenario)? {
        hash.update(packet);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn check(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err("bridge check interrupted".into())
    } else {
        Ok(())
    }
}
fn wait(cancel: &AtomicBool, mut predicate: impl FnMut() -> Result<bool>) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(8);
    while !predicate()? {
        check(cancel)?;
        if Instant::now() >= end {
            return Err("bridge readiness deadline".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    check(cancel)
}
/// This direct daemon stays inside the outer worker's owned process group.
/// It has no helper descendants; the outer deadline kills the complete group.
struct Process {
    child: Child,
    reaped: bool,
}
impl Process {
    fn start(binary: &Path, args: &[&str], output: &Path) -> Result<Self> {
        let child = Command::new(binary)
            .args(args)
            .stdout(File::create(output.join("stdout"))?)
            .stderr(File::create(output.join("stderr"))?)
            .spawn()?;
        Ok(Self {
            child,
            reaped: false,
        })
    }
    fn alive(&mut self) -> Result<bool> {
        let status = self.child.try_wait()?;
        self.reaped = status.is_some();
        Ok(status.is_none())
    }
    fn mapped(&mut self, name: &str) -> Result<bool> {
        if !self.alive()? {
            return Err("bridge exited before readiness".into());
        }
        Ok(
            fs::read_to_string(format!("/proc/{}/maps", self.child.id()))?
                .contains(&format!("/{name}\n")),
        )
    }
    fn shutdown(&mut self) -> Result<()> {
        if self.reaped {
            return Err("bridge exited before requested shutdown".into());
        }
        kill_process(Pid::from_child(&self.child), Signal::TERM)?;
        let end = Instant::now() + Duration::from_secs(4);
        loop {
            if let Some(status) = self.child.try_wait()? {
                self.reaped = true;
                return if status.success() {
                    Ok(())
                } else {
                    Err(format!("bridge shutdown: {status}").into())
                };
            }
            if Instant::now() >= end {
                return Err("bridge shutdown exceeded four seconds".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
fn write_json(path: &Path, value: &Value) -> Result<()> {
    publication::Publication::write(path, value)?.finish(&AtomicBool::new(false))
}
/// Refuse loopback setup unless the parent supplied a distinct namespace.
pub fn validate_namespace(parent: &Path, current: &Path) -> Result<()> {
    if parent == current {
        Err("loopback setup requires a private network namespace".into())
    } else {
        Ok(())
    }
}
fn worker(options: &Options, cancel: &AtomicBool) -> Result<publication::Publication> {
    let parent =
        std::env::var_os("BRIDGE_PARENT_NETWORK").ok_or("missing bridge parent network")?;
    let current = fs::read_link("/proc/self/ns/net")?;
    validate_namespace(Path::new(&parent), &current)?;
    let prefix = std::env::var("OPENPILOT_PREFIX")?;
    if !prefix.starts_with("rust-probe-bridge-") || prefix.contains('/') {
        return Err("invalid private bridge prefix".into());
    }
    let namespace = PathBuf::from("/dev/shm").join(format!("msgq_{prefix}"));
    if !namespace.is_dir() {
        return Err("missing bridge namespace owner".into());
    }
    let setup = CommandSpec::new(
        vec![
            "ip".into(),
            "link".into(),
            "set".into(),
            "lo".into(),
            "up".into(),
        ],
        Duration::from_secs(8),
    );
    check(cancel)?;
    // The outer unshare worker group owns this helper too. A nested group
    // would escape cleanup when the outer runner kills the worker on cancel.
    let setup = NativeRunner.run_in_current_group(&setup)?;
    check(cancel)?;
    if setup.code != 0 {
        return Err(format!(
            "private loopback setup: {}",
            String::from_utf8_lossy(&setup.stderr)
        )
        .into());
    }
    let scenario = options.scenario.as_deref().ok_or("missing scenario")?;
    match scenario {
        "outgoing" => traffic::outgoing(&options.binary, &options.output, &namespace, cancel)?,
        "incoming" => traffic::incoming(&options.binary, &options.output, &namespace, cancel)?,
        _ => return Err("unknown bridge scenario".into()),
    }
    check(cancel)?;
    publication::Publication::write(
        &options.output.join("scenario.json"),
        &json!({"digest": packet_digest(scenario)?, "network_namespace": identity::path_value(&current)}),
    )
}
fn isolated_case(
    binary: &Path,
    scenario: &str,
    output: &Path,
    worker: &Path,
    cancel: &AtomicBool,
) -> Result<Value> {
    let namespace = tempfile::Builder::new()
        .prefix("msgq_rust-probe-bridge-")
        .tempdir_in("/dev/shm")?;
    let prefix = namespace
        .path()
        .file_name()
        .and_then(|v| v.to_str())
        .and_then(|v| v.strip_prefix("msgq_"))
        .ok_or("invalid namespace name")?;
    let parent = fs::read_link("/proc/self/ns/net")?;
    let mut argv: Vec<OsString> = vec!["unshare".into()];
    if !rustix::process::geteuid().is_root() {
        argv.extend(["--user".into(), "--map-root-user".into()]);
    }
    argv.extend([
        "--net".into(),
        worker.as_os_str().to_owned(),
        "--binary".into(),
        binary.as_os_str().to_owned(),
        "--output".into(),
        output.as_os_str().to_owned(),
        "--scenario".into(),
        scenario.into(),
    ]);
    let mut spec = CommandSpec::new_os(argv, Duration::from_secs(60));
    spec.set_os_env(std::collections::BTreeMap::from([
        ("OPENPILOT_PREFIX".into(), prefix.into()),
        (
            "BRIDGE_PARENT_NETWORK".into(),
            parent.as_os_str().to_owned(),
        ),
    ]));
    write_json(
        &output.join("invocation.json"),
        &identity::invocation(binary, scenario, &parent),
    )?;
    let result = NativeRunner.run_controlled(&spec, Instant::now() + spec.timeout, cancel)?;
    fs::write(output.join("namespace.stdout"), &result.stdout)?;
    fs::write(output.join("namespace.stderr"), &result.stderr)?;
    if result.code != 0 {
        return Err(format!(
            "bridge {scenario} worker failed: {}: {}",
            result.code,
            String::from_utf8_lossy(&result.stderr)
        )
        .into());
    }
    let record: Value = serde_json::from_slice(&fs::read(output.join("scenario.json"))?)?;
    let child = record["network_namespace"]
        .as_str()
        .ok_or("missing child namespace")?;
    validate_namespace(&parent, Path::new(child))?;
    if record["digest"] != packet_digest(scenario)? {
        return Err("bridge packet digest mismatch".into());
    }
    Ok(record["digest"].clone())
}
pub fn compare_reports(expected: &Value, actual: &Value) -> Result<()> {
    if actual.as_object().is_none_or(|value| value.len() != 2) {
        return Err("unexpected bridge report groups".into());
    }
    for label in ["source", "rust"] {
        let value = actual[label]
            .as_object()
            .ok_or("missing bridge report group")?;
        if value.len() != 2 {
            return Err("unexpected bridge scenario count".into());
        }
        for scenario in SCENARIOS {
            if value.get(scenario) != Some(&Value::String(packet_digest(scenario)?)) {
                return Err("bridge report packet digest mismatch".into());
            }
        }
    }
    if expected != actual {
        return Err("original/native bridge controller reports differ".into());
    }
    Ok(())
}
pub fn run(options: &Options, executable: &Path, cancel: &AtomicBool) -> Result<Value> {
    if options.scenario.is_some() {
        return Err("public run does not accept worker scenario".into());
    }
    let original = options.original.as_ref().ok_or("missing source bridge")?;
    // Reject reused output before starting children, preventing stale success.
    if let Some(parent) = options.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&options.output)?;
    check(cancel)?;
    let binaries = [original.as_path(), options.binary.as_path(), executable];
    let identities = binaries
        .iter()
        .map(|path| identity::Input::capture(path))
        .collect::<Vec<_>>();
    let mut records = serde_json::Map::new();
    for (label, binary) in [("source", original), ("rust", &options.binary)] {
        let mut rows = serde_json::Map::new();
        for scenario in SCENARIOS {
            check(cancel)?;
            for input in &identities {
                input.verify_before()?;
            }
            let output = options.output.join(label).join(scenario);
            fs::create_dir_all(&output)?;
            rows.insert(
                scenario.into(),
                isolated_case(binary, scenario, &output, executable, cancel)?,
            );
        }
        records.insert(label.into(), Value::Object(rows));
    }
    let records = Value::Object(records);
    if records["source"] != records["rust"] {
        return Err("source/native bridge traffic differs".into());
    }
    check(cancel)?;
    let hashes = identities
        .iter()
        .map(identity::Input::verified_sha256)
        .collect::<Result<Vec<_>>>()?;
    write_json(
        &options.output.join("provenance.json"),
        &json!({"controller_source_sha256":SOURCE_SHA,"original_sha256":hashes[0],"native_sha256":hashes[1],"controller_sha256":hashes[2]}),
    )?;
    Ok(records)
}
pub fn cli() -> Result<()> {
    let Some(options) = parse(&std::env::args_os().skip(1).collect::<Vec<_>>())? else {
        println!("usage: check-bridge [--original ORIGINAL] --binary BINARY --output OUTPUT");
        return Ok(());
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let signals = [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM]
        .map(|signal| signal_hook::flag::register(signal, cancel.clone()))
        .into_iter()
        .collect::<std::io::Result<Vec<_>>>()?;
    let mut publication = None;
    let result = (|| {
        if options.scenario.is_some() {
            publication = Some(worker(&options, &cancel)?);
            return check(&cancel);
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let source = fs::read(root.join("rust/tools/check_bridge.py"))?;
        if format!("{:x}", Sha256::digest(&source)) != SOURCE_SHA {
            return Err("bridge original controller source drift".into());
        }
        let report = run(&options, &std::env::current_exe()?, &cancel)?;
        let mut stdout = std::io::stdout().lock();
        writeln!(stdout, "{PASS}")?;
        stdout.flush()?;
        check(&cancel)?;
        publication = Some(publication::Publication::write(
            &options.output.join("result.json"),
            &report,
        )?);
        check(&cancel)
    })();
    for signal in signals {
        signal_hook::low_level::unregister(signal);
    }
    // Observe a signal latched during publication or immediately before handler
    // removal. On any failure the guard rolls back only its own report inode.
    result.and_then(|()| {
        check(&cancel)?;
        publication
            .ok_or("missing bridge report publication")?
            .finish(&cancel)
    })
}
