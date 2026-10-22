//! Bounded native controller for the seven original paramsd startup cases.
//! The pre-retirement hosted gate independently captures the original seeds
//! and result files; this controller never executes a source oracle.
use crate::{
    paramsd_startup_fixture::{self, Case},
    seal::Result,
};
use capnp::{message::ReaderOptions, serialize};
use openpilot_cereal::log_capnp::event;
use rustix::process::{kill_process, kill_process_group, Pid, Signal};
use serde_json::{json, Value};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::{fs::MetadataExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
mod identity;
use identity::{absent, Bound, Directory};

const STARTUP: Duration = Duration::from_secs(10);
const SHUTDOWN: Duration = Duration::from_secs(5);
const OUTPUT_LIMIT: u64 = 1_048_576;
const ORIGINAL: &str = "rust/tools/check_paramsd_startup.py";
const SOURCES: &[&str] = &[
    "rust/Cargo.lock",
    "rust/crates/dev-tools/Cargo.toml",
    "rust/crates/dev-tools/src/paramsd_fixture_generate.rs",
    "rust/crates/dev-tools/src/paramsd_startup_fixture.rs",
    "rust/crates/dev-tools/src/paramsd_startup_check.rs",
    "rust/crates/dev-tools/src/paramsd_startup_check/identity.rs",
    "rust/crates/dev-tools/src/bin/check_paramsd_startup_native.rs",
    "rust/crates/paramsd/src/main.rs",
    "rust/crates/paramsd/src/runtime.rs",
    "rust/crates/paramsd/src/cache.rs",
    "rust/crates/paramsd/src/wire.rs",
    "rust/crates/paramsd/src/estimator.rs",
    "rust/crates/paramsd/native/filter.cc",
    "rust/crates/paramsd/native/scheduler.cc",
    "rust/crates/paramsd/native/model.cc",
    "openpilot/cereal/log.capnp",
    "opendbc_repo/opendbc/car/car.capnp",
];

pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub evidence: PathBuf,
}

struct Signals {
    cancelled: Arc<AtomicBool>,
    ids: Vec<signal_hook::SigId>,
}
impl Signals {
    fn new() -> Result<Self> {
        let mut signals = Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            ids: Vec::new(),
        };
        for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            signals.ids.push(signal_hook::flag::register(
                signal,
                Arc::clone(&signals.cancelled),
            )?);
        }
        Ok(signals)
    }
    fn check(&self) -> Result<()> {
        if self.cancelled.load(Ordering::SeqCst) {
            return Err("paramsd startup controller cancelled".into());
        }
        Ok(())
    }
}
impl Drop for Signals {
    fn drop(&mut self) {
        for id in self.ids.drain(..) {
            signal_hook::low_level::unregister(id);
        }
    }
}

struct Daemon {
    child: Child,
    reaped: bool,
}
impl Daemon {
    fn poll(&mut self) -> Result<Option<ExitStatus>> {
        let status = self.child.try_wait()?;
        if status.is_some() {
            self.reaped = true;
        }
        Ok(status)
    }
    fn wait(&mut self, timeout: Duration, signals: &Signals, output: &Path) -> Result<ExitStatus> {
        let start = Instant::now();
        loop {
            signals.check()?;
            check_logs(output)?;
            if let Some(status) = self.poll()? {
                return Ok(status);
            }
            if start.elapsed() >= timeout {
                return Err("paramsd startup child deadline exceeded".into());
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}
impl Drop for Daemon {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = kill_process_group(Pid::from_child(&self.child), Signal::KILL);
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn require(condition: bool, message: &str) -> Result<()> {
    if !condition {
        return Err(message.into());
    }
    Ok(())
}
fn create(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn write_json(path: &Path, value: &Value) -> Result<()> {
    create(path, &serde_json::to_vec_pretty(value)?)
}
fn bounded(path: &Path) -> Result<Vec<u8>> {
    let file = File::open(path)?;
    require(file.metadata()?.is_file(), "nonregular startup observation")?;
    let mut bytes = Vec::new();
    file.take(OUTPUT_LIMIT + 1).read_to_end(&mut bytes)?;
    require(
        bytes.len() as u64 <= OUTPUT_LIMIT,
        "startup observation exceeded 1 MiB",
    )?;
    Ok(bytes)
}
fn check_logs(output: &Path) -> Result<()> {
    for name in ["stdout.log", "stderr.log"] {
        require(
            fs::metadata(output.join(name))?.len() <= OUTPUT_LIMIT,
            "startup log exceeded 1 MiB",
        )?;
    }
    Ok(())
}

/// Validate the original migration assertion using the complete framed packet.
/// Hosted comparison additionally checks every decoded field against pycapnp.
pub fn validate_migration(bytes: &[u8]) -> Result<()> {
    let mut cursor = bytes;
    let message = serialize::read_message(&mut cursor, ReaderOptions::new())?;
    require(cursor.is_empty(), "trailing migration packet data")?;
    let root = message.get_root::<event::Reader<'_>>()?;
    require(
        root.get_log_mono_time() > 0 && !root.get_valid(),
        "migration Event metadata changed",
    )?;
    let event::LiveParameters(parameters) = root.which()? else {
        return Err("migration packet service changed".into());
    };
    let parameters = parameters?;
    require(
        parameters.get_valid() && parameters.get_steer_ratio().to_bits() == 17_f32.to_bits(),
        "migration ratio or validity changed",
    )?;
    require(
        parameters.get_stiffness_factor().to_bits() == 0.7_f32.to_bits()
            && parameters.get_angle_offset_average_deg().to_bits() == 2_f32.to_bits(),
        "migration values changed",
    )
}

fn capture_maps(daemon: &Daemon, binary: &Path, output: &Path) -> Result<()> {
    require(
        fs::read_link(format!("/proc/{}/exe", daemon.child.id()))? == binary,
        "startup child executable changed",
    )?;
    let maps = fs::read(format!("/proc/{}/maps", daemon.child.id()))?;
    let text = std::str::from_utf8(&maps)?;
    require(
        !["libpython", "ekf_sym_pyx", "libcar.so"]
            .iter()
            .any(|name| text.contains(name)),
        "startup child loaded a Python/oracle provider",
    )?;
    create(&output.join("maps.txt"), &maps)
}

fn run_case(binary: &Path, case: &Case, output: &Path, signals: &Signals) -> Result<Value> {
    fs::create_dir(output)?;
    write_json(&output.join("input.json"), &serde_json::to_value(case)?)?;
    let private = tempfile::Builder::new()
        .prefix("private-")
        .tempdir_in(output)?;
    let shm = tempfile::Builder::new()
        .prefix("msgq_rust-probe-params-")
        .tempdir_in("/dev/shm")?;
    let prefix = shm
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("msgq_"))
        .ok_or("invalid startup namespace")?;
    let params_root = private.path().join("params");
    let memory_root = private.path().join("memory");
    let params = params_root.join(prefix);
    let memory = memory_root.join(prefix);
    fs::create_dir_all(&params)?;
    fs::create_dir_all(&memory)?;
    let position = memory.join("LastGPSPosition");
    create(&position, b"previous")?;
    for (key, value) in &case.seed {
        create(&params.join(key), value)?;
    }
    let mut command = Command::new(binary);
    command
        .arg("--memory-root")
        .arg(&memory_root)
        .env("OPENPILOT_PREFIX", prefix)
        .env("PARAMS_ROOT", &params_root)
        .env("DEBUG", "1")
        .env("REPLAY", "1")
        .env_remove("ZMQ")
        .env_remove("CEREAL_FAKE")
        .stdin(Stdio::null())
        .stdout(Stdio::from(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output.join("stdout.log"))?,
        ))
        .stderr(Stdio::from(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output.join("stderr.log"))?,
        ))
        .process_group(0);
    signals.check()?;
    let mut daemon = Daemon {
        child: command.spawn()?,
        reaped: false,
    };
    drop(command);
    write_json(
        &output.join("process.json"),
        &json!({"pid":daemon.child.id(), "prefix":prefix,
        "argv":[binary.to_str().ok_or("non-UTF-8 binary path")?, "--memory-root", memory_root.to_str().ok_or("non-UTF-8 memory path")?],
        "startup_seconds":10,"shutdown_seconds":5}),
    )?;
    let failure = matches!(case.name, "malformed-car" | "wrong-covariance-dimensions");
    let shutdown;
    let status;
    if failure {
        status = daemon.wait(STARTUP, signals, output)?;
        shutdown = None;
        require(status.code() == Some(1), "startup failure did not exit 1")?;
    } else {
        let start = Instant::now();
        loop {
            signals.check()?;
            check_logs(output)?;
            require(
                daemon.poll()?.is_none(),
                "paramsd exited before startup readiness",
            )?;
            if shm.path().join("liveParameters").exists()
                && (case.name == "blocked-car" || !position.exists())
            {
                break;
            }
            require(
                start.elapsed() < STARTUP,
                "paramsd startup exceeded ten seconds",
            )?;
            thread::sleep(Duration::from_millis(10));
        }
        capture_maps(&daemon, binary, output)?;
        let start = Instant::now();
        signals.check()?;
        kill_process(Pid::from_child(&daemon.child), Signal::TERM)?;
        status = daemon.wait(SHUTDOWN, signals, output)?;
        shutdown = Some(start.elapsed().as_secs_f64());
        require(
            status.code() == Some(0),
            "paramsd SIGTERM shutdown did not exit 0",
        )?;
    }
    check_logs(output)?;
    signals.check()?;
    // Keep every final key byte, independent of the controller's assertions.
    let snapshot = output.join("params");
    fs::create_dir(&snapshot)?;
    let mut files = Vec::new();
    for entry in fs::read_dir(&params)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "non-UTF-8 Params key")?;
        require(entry.file_type()?.is_file(), "nonregular final Params key")?;
        create(&snapshot.join(&name), &bounded(&entry.path())?)?;
        files.push(name);
    }
    files.sort();
    if position.exists() {
        create(&output.join("LastGPSPosition.bin"), &bounded(&position)?)?;
    }
    match case.name {
        "blocked-car" => require(
            bounded(&position)? == b"previous",
            "blocked-car changed LastGPSPosition",
        )?,
        "invalid-json" | "migration" => require(
            params.join("LiveParameters").exists(),
            "legacy migration key lost",
        )?,
        "wrong-fields" => require(
            !params.join("LiveParameters").exists(),
            "wrong-fields was retained",
        )?,
        "malformed-cache" => require(
            !params.join("LiveParametersV2").exists(),
            "malformed cache was retained",
        )?,
        "wrong-covariance-dimensions" => {
            require(
                bounded(&params.join("LiveParametersV2"))? == case.seed["LiveParametersV2"],
                "wrong-dimension cache changed",
            )?;
            require(
                String::from_utf8(bounded(&output.join("stderr.log"))?)?.contains("dimensions"),
                "dimension error diagnostic missing",
            )?;
        }
        _ => {}
    }
    if case.name == "migration" {
        let bytes = bounded(&params.join("LiveParametersV2"))?;
        validate_migration(&bytes)?;
        create(&output.join("migrated.bin"), &bytes)?;
    }
    let original_shape =
        json!({"pass":true,"scenario":case.name,"exit":status.code(),"files":files});
    write_json(&output.join("results.json"), &original_shape)?;
    write_json(
        &output.join("lifecycle.json"),
        &json!({"exit":status.code(),"signal":if failure {None} else {Some("SIGTERM")},"shutdown_seconds":shutdown}),
    )?;
    Ok(original_shape)
}

fn check(signals: &Signals, directory: &Directory, bounds: &[Bound], missing: &Path) -> Result<()> {
    signals.check()?;
    directory.verify()?;
    for bound in bounds {
        bound.verify()?;
    }
    if !missing.as_os_str().is_empty() {
        require(
            absent(missing)?,
            "absent original startup checker reappeared",
        )?;
    }
    Ok(())
}

// Remove only our own receipt on failed finalization; preserve any replacement.
struct Receipt {
    path: PathBuf,
    dev: u64,
    ino: u64,
    committed: bool,
}
impl Drop for Receipt {
    fn drop(&mut self) {
        if !self.committed
            && fs::symlink_metadata(&self.path)
                .is_ok_and(|m| (m.dev(), m.ino()) == (self.dev, self.ino))
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub fn run(options: &Options) -> Result<()> {
    let signals = Signals::new()?;
    let root = options.root.canonicalize()?;
    let binary_path = options.binary.canonicalize()?;
    // The caller supplies exact regular paths; canonicalization may not admit
    // a symlink in the executable's final component.
    require(
        !fs::symlink_metadata(&options.binary)?
            .file_type()
            .is_symlink(),
        "startup binary symlink",
    )?;
    let mut binary = Bound::open(&binary_path, None)?;
    binary.native_elf()?;
    let mut bounds = vec![binary];
    for source in SOURCES {
        bounds.push(Bound::open(&root.join(source), None)?);
    }
    bounds.push(Bound::open(
        &root.join("rust/tools/paramsd_fixture.py"),
        Some(paramsd_startup_fixture::FIXTURE_SHA256),
    )?);
    let missing = if absent(&root.join(ORIGINAL))? {
        root.join(ORIGINAL)
    } else {
        bounds.push(Bound::open(
            &root.join(ORIGINAL),
            Some(paramsd_startup_fixture::ORIGINAL_SHA256),
        )?);
        PathBuf::new()
    };
    let directory = Directory::create(&options.evidence)?;
    write_json(
        &options.evidence.join("inputs.json"),
        &serde_json::to_value(paramsd_startup_fixture::cases())?,
    )?;
    bounds.push(Bound::open(&options.evidence.join("inputs.json"), None)?);
    let mut results = Vec::new();
    for case in paramsd_startup_fixture::cases() {
        check(&signals, &directory, &bounds, &missing)?;
        let output = options.evidence.join(case.name);
        results.push(run_case(&binary_path, &case, &output, &signals)?);
        for entry in fs::read_dir(&output)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                bounds.push(Bound::open(&entry.path(), None)?);
            }
        }
        for entry in fs::read_dir(output.join("params"))? {
            bounds.push(Bound::open(&entry?.path(), None)?);
        }
    }
    check(&signals, &directory, &bounds, &missing)?;
    let bindings: Vec<_> = bounds
        .iter()
        .map(|bound| json!({"path":bound.path,"sha256":bound.hash}))
        .collect();
    let report = json!({"pass":true,"scenarios":results,"bindings":bindings,
        "historical_startup_sha256":paramsd_startup_fixture::ORIGINAL_SHA256,
        "startup_original_absent":!missing.as_os_str().is_empty(),
        "startup_seconds":10,"shutdown_seconds":5});
    let path = options.evidence.join("results.json");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    let metadata = file.metadata()?;
    let mut receipt = Receipt {
        path: path.clone(),
        dev: metadata.dev(),
        ino: metadata.ino(),
        committed: false,
    };
    file.write_all(&serde_json::to_vec_pretty(&report)?)?;
    file.sync_all()?;
    let published = Bound::open(&path, None)?;
    require(
        (metadata.dev(), metadata.ino())
            == (
                fs::symlink_metadata(&path)?.dev(),
                fs::symlink_metadata(&path)?.ino(),
            ),
        "startup result identity changed",
    )?;
    check(&signals, &directory, &bounds, &missing)?;
    published.verify()?;
    File::open(&options.evidence)?.sync_all()?;
    signals.check()?;
    receipt.committed = true;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn migration(ratio: f32, valid: bool) -> Vec<u8> {
        let mut message = capnp::message::Builder::new_default();
        let mut root = message.init_root::<event::Builder<'_>>();
        root.set_log_mono_time(1);
        root.set_valid(valid);
        let mut out = root.init_live_parameters();
        out.set_valid(true);
        out.set_steer_ratio(ratio);
        out.set_stiffness_factor(0.7);
        out.set_angle_offset_average_deg(2.);
        serialize::write_message_to_words(&message)
    }

    #[test]
    fn migration_requires_exact_float_bits_metadata_and_framing() -> Result<()> {
        validate_migration(&migration(17., false))?;
        assert!(
            validate_migration(&migration(f32::from_bits(17_f32.to_bits() + 1), false)).is_err()
        );
        assert!(validate_migration(&migration(17., true)).is_err());
        let mut trailing = migration(17., false);
        trailing.extend_from_slice(&[0; 8]);
        assert!(validate_migration(&trailing).is_err());
        assert!(validate_migration(&[1, 2]).is_err());
        Ok(())
    }

    #[test]
    fn finalization_rejects_cancel_source_reappearance_and_foreign_directory() -> Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("evidence");
        let directory = Directory::create(&path)?;
        let missing = root.path().join("original.py");
        let signals = Signals {
            cancelled: Arc::new(AtomicBool::new(false)),
            ids: Vec::new(),
        };
        check(&signals, &directory, &[], &missing)?;
        signals.cancelled.store(true, Ordering::SeqCst);
        assert!(check(&signals, &directory, &[], &missing).is_err());
        signals.cancelled.store(false, Ordering::SeqCst);
        fs::write(&missing, b"reappeared")?;
        assert!(check(&signals, &directory, &[], &missing).is_err());
        fs::remove_file(&missing)?;
        fs::rename(&path, root.path().join("old"))?;
        fs::create_dir(&path)?;
        assert!(check(&signals, &directory, &[], &missing).is_err());
        Ok(())
    }

    #[test]
    fn receipt_rollback_preserves_foreign_replacement() -> Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("receipt");
        create(&path, b"ours")?;
        let metadata = fs::metadata(&path)?;
        let guard = Receipt {
            path: path.clone(),
            dev: metadata.dev(),
            ino: metadata.ino(),
            committed: false,
        };
        fs::rename(&path, root.path().join("old"))?;
        create(&path, b"foreign")?;
        drop(guard);
        assert_eq!(fs::read(&path)?, b"foreign");
        let metadata = fs::metadata(&path)?;
        drop(Receipt {
            path: path.clone(),
            dev: metadata.dev(),
            ino: metadata.ino(),
            committed: false,
        });
        assert!(!path.exists());
        Ok(())
    }
}
