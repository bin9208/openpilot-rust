//! Native production-updater lifecycle controller. The unchanged Python
//! controller remains the independent comparison until cloud admission.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
    updated_fixture,
};
use rustix::process::{
    kill_process, kill_process_group, waitid, Pid, Signal, WaitId, WaitIdOptions,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    env,
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

const ORIGINAL_SHA256: &str = "82be3830f8e468c32abe583c197cdf2777cd5fb748a795a8c70247a7dcd00059";
const READY: Duration = Duration::from_secs(15);
const DUPLICATE: Duration = Duration::from_secs(5);
const TERM: Duration = Duration::from_secs(4);
const FORCE_CLEANUP: Duration = Duration::from_secs(3);
static NEXT_PREFIX: AtomicU64 = AtomicU64::new(0);

pub struct Options {
    pub binary: PathBuf,
    pub launcher: PathBuf,
    pub fixture: PathBuf,
    pub output: PathBuf,
    pub root: PathBuf,
}

pub fn original_sha256() -> &'static str {
    ORIGINAL_SHA256
}

fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn identity(path: &Path) -> Result<(u64, u64)> {
    let metadata = fs::symlink_metadata(path)?;
    Ok((metadata.dev(), metadata.ino()))
}

struct Pin {
    path: PathBuf,
    held: File,
    dev: u64,
    ino: u64,
    sha256: String,
    max_bytes: Option<u64>,
}
impl Pin {
    fn open(path: PathBuf, expected: Option<&str>, max_bytes: Option<u64>) -> Result<Self> {
        let fd = rustix::fs::open(
            &path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        let held = File::from(fd);
        let metadata = held.metadata()?;
        require(metadata.is_file(), "updater input is not a regular file")?;
        let mut pin = Self {
            path,
            held,
            dev: metadata.dev(),
            ino: metadata.ino(),
            sha256: String::new(),
            max_bytes,
        };
        pin.sha256 = pin.hash_held()?;
        if let Some(expected) = expected {
            require(
                pin.sha256 == expected,
                "historical updater daemon source changed",
            )?;
        }
        pin.verify()?;
        Ok(pin)
    }
    fn hash_held(&mut self) -> Result<String> {
        self.held.seek(SeekFrom::Start(0))?;
        let mut hasher = Sha256::new();
        let mut bytes = [0_u8; 65536];
        let mut total = 0_u64;
        loop {
            let count = self.held.read(&mut bytes)?;
            if count == 0 {
                break;
            }
            total += u64::try_from(count)?;
            require(
                self.max_bytes.is_none_or(|max| total <= max),
                "historical updater source exceeds two MiB",
            )?;
            hasher.update(&bytes[..count]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }
    fn verify(&mut self) -> Result<()> {
        let current = fs::symlink_metadata(&self.path)?;
        let held = self.held.metadata()?;
        let hash = self.hash_held()?;
        require(
            current.is_file()
                && held.is_file()
                && (current.dev(), current.ino()) == (self.dev, self.ino)
                && (held.dev(), held.ino()) == (self.dev, self.ino)
                && hash == self.sha256,
            "updater daemon input identity or content changed",
        )
    }
}

struct Historical {
    path: PathBuf,
    parent: PathBuf,
    parent_identity: (u64, u64),
    pin: Option<Pin>,
}
impl Historical {
    fn open(root: &Path) -> Result<Self> {
        let path = root.join("rust/tools/check_updated_daemon.py");
        let parent = path
            .parent()
            .ok_or("historical updater source parent missing")?
            .to_owned();
        require(
            fs::symlink_metadata(&parent)?.is_dir(),
            "historical updater parent is not a directory",
        )?;
        let parent_identity = identity(&parent)?;
        let pin = match fs::symlink_metadata(&path) {
            Ok(_) => Some(Pin::open(
                path.clone(),
                Some(ORIGINAL_SHA256),
                Some(2 * 1024 * 1024),
            )?),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        Ok(Self {
            path,
            parent,
            parent_identity,
            pin,
        })
    }
    fn verify(&mut self) -> Result<()> {
        require(
            identity(&self.parent)? == self.parent_identity
                && fs::symlink_metadata(&self.parent)?.is_dir(),
            "historical updater source parent changed",
        )?;
        if let Some(pin) = &mut self.pin {
            pin.verify()
        } else {
            require(
                matches!(fs::symlink_metadata(&self.path), Err(error) if error.kind() == io::ErrorKind::NotFound),
                "historical updater source reappeared",
            )
        }
    }
}

fn native_elf(path: &Path) -> Result<()> {
    let mut header = [0_u8; 64];
    File::open(path)?.read_exact(&mut header)?;
    let machine: u16 = match std::env::consts::ARCH {
        "x86_64" => 62,
        "aarch64" => 183,
        _ => return Err("unsupported updater controller host architecture".into()),
    };
    require(
        &header[..4] == b"\x7fELF"
            && header[4] == 2
            && header[5] == 1
            && matches!(u16::from_le_bytes([header[16], header[17]]), 2 | 3)
            && u16::from_le_bytes([header[18], header[19]]) == machine,
        "updated daemon input is not a native host ELF",
    )
}

struct Signals {
    cancelled: Arc<AtomicBool>,
    ids: Vec<signal_hook::SigId>,
}
impl Signals {
    fn new() -> Result<Self> {
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut ids = Vec::new();
        for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            ids.push(signal_hook::flag::register(signal, Arc::clone(&cancelled))?);
        }
        Ok(Self { cancelled, ids })
    }
    fn check(&self) -> Result<()> {
        require(
            !self.cancelled.load(Ordering::Relaxed),
            "updated daemon controller cancelled",
        )
    }
    fn teardown(&mut self) {
        for id in self.ids.drain(..) {
            signal_hook::low_level::unregister(id);
        }
    }
}
impl Drop for Signals {
    fn drop(&mut self) {
        self.teardown();
    }
}

struct Receipt {
    path: PathBuf,
    held: File,
    identity: (u64, u64),
    committed: bool,
}
impl Receipt {
    fn write(path: PathBuf, bytes: &[u8]) -> Result<Self> {
        let mut held = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let metadata = held.metadata()?;
        let mut receipt = Self {
            path,
            held,
            identity: (metadata.dev(), metadata.ino()),
            committed: false,
        };
        receipt.held.write_all(bytes)?;
        receipt.held.flush()?;
        Ok(receipt)
    }
    fn owns(&self) -> bool {
        identity(&self.path).is_ok_and(|now| now == self.identity)
            && self
                .held
                .metadata()
                .is_ok_and(|held| (held.dev(), held.ino()) == self.identity)
    }
    fn commit(mut self) -> Result<()> {
        require(
            self.owns(),
            "updated daemon result receipt replaced before commit",
        )?;
        self.committed = true;
        Ok(())
    }
}
impl Drop for Receipt {
    fn drop(&mut self) {
        if !self.committed && self.owns() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

struct Completed {
    report: Value,
    output: PathBuf,
    output_identity: (u64, u64),
    inputs: [Pin; 3],
    historical: Historical,
}
impl Completed {
    fn verify(&mut self, cancelled: &Signals) -> Result<()> {
        cancelled.check()?;
        require(
            identity(&self.output)? == self.output_identity
                && fs::symlink_metadata(&self.output)?.is_dir(),
            "updated daemon output directory replaced",
        )?;
        for input in &mut self.inputs {
            input.verify()?;
        }
        self.historical.verify()
    }
    fn finish_after_teardown(
        mut self,
        signals: &mut Signals,
        after_teardown: impl FnOnce(),
    ) -> Result<Value> {
        self.verify(signals)?;
        let receipt = Receipt::write(
            self.output.join("result.json"),
            format!("{}\n", serde_json::to_string_pretty(&self.report)?).as_bytes(),
        )?;
        signals.teardown();
        after_teardown();
        self.verify(signals)?;
        receipt.commit()?;
        Ok(self.report)
    }
    fn finish(self, signals: &mut Signals) -> Result<Value> {
        self.finish_after_teardown(signals, || {})
    }
}

struct OwnedChild {
    child: Child,
    reaped: bool,
}
impl OwnedChild {
    fn exited(&self) -> Result<bool> {
        match waitid(
            WaitId::Pid(Pid::from_child(&self.child)),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        ) {
            Ok(status) => Ok(status.is_some()),
            Err(rustix::io::Errno::INTR) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
    fn alive(&mut self, log: &Path) -> Result<()> {
        if self.exited()? {
            return Err(
                format!("updated daemon exited early: {}", fs::read_to_string(log)?).into(),
            );
        }
        Ok(())
    }
    fn signal(&mut self, signal: Signal) -> Result<()> {
        self.alive(Path::new("/dev/null"))?;
        kill_process(Pid::from_child(&self.child), signal)?;
        Ok(())
    }
    fn stop(&mut self, cancel: &Signals) -> Result<i32> {
        self.signal(Signal::TERM)?;
        let start = Instant::now();
        loop {
            cancel.check()?;
            if self.exited()? {
                let _ = kill_process_group(Pid::from_child(&self.child), Signal::KILL);
                let status = self.child.wait()?;
                self.reaped = true;
                require(
                    status.code() == Some(0),
                    "updated daemon SIGTERM exit was not zero",
                )?;
                return Ok(0);
            }
            require(
                start.elapsed() < TERM,
                "updated daemon SIGTERM exceeded four seconds",
            )?;
            thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.reaped {
            // The group is owned by this controller. Reap the direct leader.
            let _ = kill_process_group(Pid::from_child(&self.child), Signal::KILL);
            let _ = self.child.kill();
            let start = Instant::now();
            while start.elapsed() < FORCE_CLEANUP {
                if self.child.try_wait().ok().flatten().is_some() {
                    self.reaped = true;
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

struct Collector {
    socket: Option<zmq::Socket>,
    _context: zmq::Context,
    path: PathBuf,
}
impl Collector {
    fn bind(prefix: &str) -> Result<Self> {
        let path = PathBuf::from(format!("/tmp/logmessage{prefix}"));
        let context = zmq::Context::new();
        let socket = context.socket(zmq::PULL)?;
        socket.set_linger(0)?;
        socket.set_rcvtimeo(100)?;
        socket.bind(&format!("ipc://{}", path.display()))?;
        Ok(Self {
            socket: Some(socket),
            _context: context,
            path,
        })
    }
    fn records(&self, cancel: &Signals) -> Result<Vec<Value>> {
        let mut records = Vec::new();
        loop {
            cancel.check()?;
            let raw = match self
                .socket
                .as_ref()
                .ok_or("collector closed")?
                .recv_bytes(0)
            {
                Ok(raw) => raw,
                Err(zmq::Error::EAGAIN) => break,
                Err(zmq::Error::EINTR) => continue,
                Err(error) => return Err(error.into()),
            };
            let (_, body) = raw.split_first().ok_or("empty updater log packet")?;
            records.push(serde_json::from_slice(body)?);
        }
        Ok(records)
    }
}
impl Drop for Collector {
    fn drop(&mut self) {
        self.socket.take();
        let _ = fs::remove_file(&self.path);
    }
}

fn wait_for(
    daemon: &mut OwnedChild,
    log: &Path,
    cancel: &Signals,
    mut predicate: impl FnMut() -> Result<bool>,
) -> Result<()> {
    let start = Instant::now();
    loop {
        cancel.check()?;
        daemon.alive(log)?;
        if predicate()? {
            return Ok(());
        }
        require(
            start.elapsed() < READY,
            "updated daemon readiness exceeded fifteen seconds",
        )?;
        thread::sleep(Duration::from_millis(20));
    }
}

fn text(path: &Path) -> Result<String> {
    Ok(fs::read_to_string(path)?)
}
fn head(path: &Path) -> Result<String> {
    let mut spec = CommandSpec::new_os(
        vec!["/usr/bin/git".into(), "rev-parse".into(), "HEAD".into()],
        Duration::from_secs(5),
    );
    spec.cwd = Some(path.to_path_buf());
    let result = NativeRunner.run(&spec)?;
    require(result.code == 0, "updater fixture Git rev-parse failed")?;
    Ok(String::from_utf8(result.stdout)?.trim().to_owned())
}

pub fn parse(args: &[OsString], current_exe: &Path) -> Result<Option<Options>> {
    let parsed = scan_os(
        args,
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--launcher", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--fixture", false),
            OptionSpec::value("--root", false),
        ],
    )?;
    if parsed.help {
        return Ok(None);
    }
    let get = |key: &str| -> Result<PathBuf> {
        Ok(PathBuf::from(
            parsed
                .values
                .get(key)
                .ok_or_else(|| format!("missing {key}"))?
                .clone(),
        ))
    };
    let binary = get("--binary")?;
    let launcher = get("--launcher")?;
    let output = get("--output")?;
    let fixture = parsed
        .values
        .get("--fixture")
        .map(PathBuf::from)
        .unwrap_or_else(|| current_exe.with_file_name("updated-fixture"));
    let root = parsed
        .values
        .get("--root")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(3)
                .unwrap()
                .to_owned()
        });
    Ok(Some(Options {
        binary,
        launcher,
        fixture,
        output,
        root,
    }))
}

fn execute(options: &Options, signals: &Signals) -> Result<Completed> {
    require(
        options.binary.is_file() && options.launcher.is_file() && options.fixture.is_file(),
        "updater daemon requires native ELF inputs",
    )?;
    require(
        !options.output.exists(),
        "updated daemon evidence output already exists",
    )?;
    let binary = fs::canonicalize(&options.binary)?;
    let launcher = fs::canonicalize(&options.launcher)?;
    let fixture = fs::canonicalize(&options.fixture)?;
    let inputs = [
        Pin::open(binary.clone(), None, None)?,
        Pin::open(launcher.clone(), None, None)?,
        Pin::open(fixture.clone(), None, None)?,
    ];
    for input in &inputs {
        native_elf(&input.path)?;
    }
    let historical = Historical::open(&options.root)?;
    let parent = options
        .output
        .parent()
        .ok_or("updated daemon output parent missing")?;
    if !parent.as_os_str().is_empty() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&options.output)?;
    let output_identity = identity(&options.output)?;
    signals.check()?;
    let temporary = tempfile::Builder::new()
        .prefix("updated-daemon-")
        .tempdir()?;
    let seed = updated_fixture::seed(temporary.path())?;
    let root = temporary.path().join("runtime");
    let mut setup = updated_fixture::setup(&root, &seed, &launcher, &fixture)?;
    let staging = PathBuf::from(
        setup.config["paths"]["staging"]
            .as_str()
            .ok_or("missing staging path")?,
    );
    let lock = setup.config["paths"]["lock"]
        .as_str()
        .ok_or("missing lock path")?;
    // A private IPC name avoids collision with other cloud tests.
    let prefix = format!(
        "updated_{}_{}",
        std::process::id(),
        NEXT_PREFIX.fetch_add(1, Ordering::Relaxed)
    );
    setup
        .environment
        .insert("OPENPILOT_PREFIX".into(), prefix.clone().into());
    setup
        .environment
        .insert("UPDATER_STAGING_ROOT".into(), staging.as_os_str().into());
    setup
        .environment
        .insert("UPDATER_LOCK_FILE".into(), lock.into());
    let collector = Collector::bind(&prefix)?;
    let params = root.join("data/params/d");
    let args = [
        OsString::from("--basedir"),
        root.join("base").into_os_string(),
        OsString::from("--system-root"),
        root.clone().into_os_string(),
        OsString::from("--launcher"),
        launcher.clone().into_os_string(),
    ];
    let log = options.output.join("daemon.log");
    let output = OpenOptions::new().write(true).create_new(true).open(&log)?;
    let mut command = Command::new(&binary);
    command
        .args(&args)
        .envs(&setup.environment)
        .stdout(Stdio::from(output.try_clone()?))
        .stderr(Stdio::from(output))
        .process_group(0);
    let mut daemon = OwnedChild {
        child: command.spawn()?,
        reaped: false,
    };
    wait_for(&mut daemon, &log, &signals, || {
        Ok(params.join("UpdaterCurrentDescription").exists()
            && params.join("UpdaterFetchAvailable").exists())
    })?;
    let mut duplicate = CommandSpec::new_os(
        std::iter::once(binary.clone().into_os_string())
            .chain(args.iter().cloned())
            .collect(),
        DUPLICATE,
    );
    duplicate.set_os_env(setup.environment.clone());
    let duplicate =
        NativeRunner.run_controlled(&duplicate, Instant::now() + DUPLICATE, &signals.cancelled)?;
    require(
        duplicate.code == 1 && String::from_utf8_lossy(&duplicate.stderr).contains("overlay lock"),
        "duplicate updated daemon did not reject overlay lock",
    )?;
    fs::write(options.output.join("duplicate.log"), &duplicate.stderr)?;
    daemon.signal(Signal::USR1)?;
    wait_for(&mut daemon, &log, &signals, || {
        Ok(
            text(&params.join("UpdaterFetchAvailable")).ok().as_deref() == Some("1")
                && text(&params.join("UpdaterState")).ok().as_deref() == Some("idle"),
        )
    })?;
    require(
        text(&params.join("UpdateAvailable"))? == "0",
        "UpdateAvailable before SIGHUP was not zero",
    )?;
    daemon.signal(Signal::HUP)?;
    wait_for(&mut daemon, &log, &signals, || {
        Ok(
            text(&params.join("UpdateAvailable")).ok().as_deref() == Some("1")
                && text(&params.join("UpdaterState")).ok().as_deref() == Some("idle"),
        )
    })?;
    let finalized = staging.join("finalized");
    require(
        finalized.join(".overlay_consistent").is_file(),
        "finalized overlay consistency marker missing",
    )?;
    let final_head = head(&finalized)?;
    let remote_head = head(&seed.remote)?;
    require(
        final_head == remote_head,
        "finalized commit differs from remote",
    )?;
    require(
        text(&finalized.join("common/version.h"))? == "#define COMMA_VERSION \"2.0\"\n",
        "finalized version header differs",
    )?;
    require(
        fs::symlink_metadata(finalized.join("version-link"))?
            .file_type()
            .is_symlink(),
        "finalized version link missing",
    )?;
    require(
        fs::metadata(finalized.join("launch_env.sh"))?
            .permissions()
            .mode()
            & 0o111
            != 0,
        "finalized launch script is not executable",
    )?;
    require(
        fs::read(params.join("UpdaterNewReleaseNotes"))?.starts_with(b"<h1>2.0</h1>"),
        "new release notes missing",
    )?;
    require(
        params.join("UpdaterLastFetchTime").exists(),
        "last fetch time missing",
    )?;
    let status = daemon.stop(&signals)?;
    if fs::metadata(&log)?.len() == 0 {
        fs::write(&log, b"<empty>\n")?;
    }
    let records = collector.records(&signals)?;
    let messages = records
        .iter()
        .filter_map(|row| row["msg"].as_str())
        .collect::<Vec<_>>();
    require(
        messages.contains(&"caught SIGUSR1, checking for updates"),
        "SIGUSR1 log missing",
    )?;
    require(
        messages.contains(&"caught SIGHUP, attempting to downloading update"),
        "SIGHUP log missing",
    )?;
    fs::write(
        options.output.join("records.json"),
        serde_json::to_vec_pretty(&records)?,
    )?;
    let mut params_json = serde_json::Map::new();
    for entry in fs::read_dir(&params)? {
        let path = entry?.path();
        if path.is_file() {
            params_json.insert(
                path.file_name()
                    .and_then(OsStr::to_str)
                    .ok_or("invalid Params name")?
                    .into(),
                json!(text(&path)?),
            );
        }
    }
    fs::write(
        options.output.join("params.json"),
        serde_json::to_vec_pretty(&params_json)?,
    )?;
    fs::copy(
        root.join("overlay-commands.jsonl"),
        options.output.join("overlay-commands.jsonl"),
    )?;
    let report = json!({"passed":true,"duplicate_lock_exit":duplicate.code,"sigterm_exit":status,"sigusr1_checked":true,"sighup_fetched":true,"finalized_commit":remote_head,"actual_mount_or_sudo":false,"device_access":false});
    signals.check()?;
    let mut completed = Completed {
        report,
        output: options.output.clone(),
        output_identity,
        inputs,
        historical,
    };
    completed.verify(signals)?;
    fs::write(
        options.output.join("input-identities.json"),
        serde_json::to_vec_pretty(
            &json!({"binary":&completed.inputs[0].sha256,"launcher":&completed.inputs[1].sha256,"fixture":&completed.inputs[2].sha256,"historical_source_present":completed.historical.pin.is_some(),"unchanged_after_run":true}),
        )?,
    )?;
    Ok(completed)
}

pub fn run(options: &Options) -> Result<Value> {
    let mut signals = Signals::new()?;
    let mut completed = execute(options, &signals)?;
    completed.verify(&signals)?;
    {
        let mut stdout = io::stdout().lock();
        stdout.write_all(b"PASS production updated: exclusive lock, SIGUSR1 check, SIGHUP fetch/finalize, Params/logs, symlink/mode and SIGTERM lifecycle\n")?;
        stdout.flush()?;
    }
    completed.finish(&mut signals)
}

pub fn cli() -> Result<()> {
    let Some(options) = parse(
        &env::args_os().skip(1).collect::<Vec<_>>(),
        &env::current_exe()?,
    )?
    else {
        io::stdout().write_all(b"Usage: check-updated-daemon --binary PATH --launcher PATH --output DIR [--fixture PATH] [--root ROOT]\n")?;
        return Ok(());
    };
    run(&options)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn historical_source_rejects_mutation_alias_and_reappearance() {
        let temporary = tempfile::tempdir().unwrap();
        let tools = temporary.path().join("rust/tools");
        fs::create_dir_all(&tools).unwrap();
        let path = tools.join("check_updated_daemon.py");
        let original =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/check_updated_daemon.py");
        fs::copy(&original, &path).unwrap();
        let mut present = Historical::open(temporary.path()).unwrap();
        present.verify().unwrap();
        fs::write(&path, b"changed").unwrap();
        assert!(present.verify().is_err());
        fs::remove_file(&path).unwrap();
        symlink(&original, &path).unwrap();
        assert!(Historical::open(temporary.path()).is_err());
        fs::remove_file(&path).unwrap();
        assert!(Command::new("/usr/bin/mkfifo")
            .arg(&path)
            .status()
            .unwrap()
            .success());
        assert!(Historical::open(temporary.path()).is_err());
        fs::remove_file(&path).unwrap();
        let mut absent = Historical::open(temporary.path()).unwrap();
        absent.verify().unwrap();
        fs::write(&path, b"reappeared").unwrap();
        assert!(absent.verify().is_err());
        fs::remove_dir_all(&tools).unwrap();
        assert!(Historical::open(temporary.path()).is_err());
    }

    #[test]
    fn result_receipt_rolls_back_only_its_own_inode() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("result.json");
        let pending = Receipt::write(path.clone(), b"pending").unwrap();
        fs::rename(&path, temporary.path().join("old")).unwrap();
        fs::write(&path, b"foreign").unwrap();
        assert!(!pending.owns());
        drop(pending);
        assert_eq!(fs::read(&path).unwrap(), b"foreign");
        assert!(Receipt::write(path, b"no clobber").is_err());
    }

    fn completed_for_test(root: &Path, output: PathBuf) -> Completed {
        let tools = root.join("rust/tools");
        fs::create_dir_all(&tools).unwrap();
        let source =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/check_updated_daemon.py");
        if !tools.join("check_updated_daemon.py").exists() {
            fs::copy(source, tools.join("check_updated_daemon.py")).unwrap();
        }
        fs::create_dir(&output).unwrap();
        Completed {
            report: json!({"passed":true}),
            output_identity: identity(&output).unwrap(),
            output,
            inputs: std::array::from_fn(|_| {
                Pin::open(PathBuf::from("/bin/true"), None, None).unwrap()
            }),
            historical: Historical::open(root).unwrap(),
        }
    }

    #[test]
    fn finalization_rejects_late_source_drift_and_preserves_foreign_receipt() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let source = root.join("rust/tools/check_updated_daemon.py");
        let mut signals = Signals {
            cancelled: Arc::new(AtomicBool::new(false)),
            ids: Vec::new(),
        };
        let drift = completed_for_test(root, root.join("drift"));
        assert!(drift
            .finish_after_teardown(&mut signals, || fs::write(&source, b"late drift").unwrap())
            .is_err());
        assert!(!root.join("drift/result.json").exists());
        fs::remove_file(&source).unwrap();
        let foreign = completed_for_test(root, root.join("foreign"));
        let receipt = root.join("foreign/result.json");
        assert!(foreign
            .finish_after_teardown(&mut signals, || {
                fs::rename(&receipt, root.join("moved-owned")).unwrap();
                fs::write(&receipt, b"foreign replacement").unwrap();
            })
            .is_err());
        assert_eq!(fs::read(receipt).unwrap(), b"foreign replacement");
        let cancelled = completed_for_test(root, root.join("cancelled"));
        let flag = Arc::clone(&signals.cancelled);
        assert!(cancelled
            .finish_after_teardown(&mut signals, || flag.store(true, Ordering::Relaxed))
            .is_err());
        assert!(!root.join("cancelled/result.json").exists());
    }

    #[test]
    fn exited_leader_keeps_group_owned_until_descendant_is_killed() {
        let temporary = tempfile::tempdir().unwrap();
        let pidfile = temporary.path().join("descendant-pid");
        let child = Command::new("/bin/sh")
            .args(["-c", "sleep 30 & echo $! > \"$1\"", "sh"])
            .arg(&pidfile)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .unwrap();
        let mut owned = OwnedChild {
            child,
            reaped: false,
        };
        let start = Instant::now();
        while !pidfile.exists() || !owned.exited().unwrap() {
            assert!(start.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(10));
        }
        let pid = fs::read_to_string(&pidfile)
            .unwrap()
            .trim()
            .parse::<u32>()
            .unwrap();
        let before = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
        assert_ne!(
            before.split_whitespace().nth(2),
            Some("Z"),
            "descendant exited before owner cleanup"
        );
        assert!(owned.alive(Path::new("/dev/null")).is_err());
        drop(owned);
        let start = Instant::now();
        loop {
            let state = fs::read_to_string(format!("/proc/{pid}/stat"));
            if state.is_err()
                || state
                    .as_ref()
                    .is_ok_and(|text| text.split_whitespace().nth(2) == Some("Z"))
            {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(3),
                "owned descendant survived leader exit"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}
