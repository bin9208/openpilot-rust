//! Live native driving-daemon comparison against an independent original-only
//! observation. The C++ transport peer is the unchanged original test peer;
//! no Python runs in this controller.
use crate::{
    driving_daemon_reference::{compare_packet, Original},
    driving_logging_check::{Collector, DrivingScenario},
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
    services_header,
};
use openpilot_cereal::car_capnp::car_params;
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    process::{kill_process, kill_process_group, Pid, Signal},
    time::{clock_gettime, ClockId},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs::{self, File, Metadata},
    io::{Read, Write},
    os::{
        fd::OwnedFd,
        unix::{
            fs::{FileExt, MetadataExt},
            process::CommandExt,
        },
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

mod model_source;

const LINE_WAIT: Duration = Duration::from_secs(60);
const SHUTDOWN: Duration = Duration::from_secs(5);
const LOG_LIMIT: usize = 16 * 1024 * 1024;
const PEER_FILES: [&str; 10] = [
    "rust/tools/driving_daemon_peer.cc",
    "msgq_repo/msgq/ipc.cc",
    "msgq_repo/msgq/event.cc",
    "msgq_repo/msgq/impl_msgq.cc",
    "msgq_repo/msgq/impl_fake.cc",
    "msgq_repo/msgq/msgq.cc",
    "msgq_repo/msgq/visionipc/visionipc.cc",
    "msgq_repo/msgq/visionipc/visionipc_client.cc",
    "msgq_repo/msgq/visionipc/visionipc_server.cc",
    "msgq_repo/msgq/visionipc/visionbuf.cc",
];
const OUTPUTS: [&str; 3] = ["modelV2", "drivingModelData", "cameraOdometry"];
const TOPICS: [&str; 9] = [
    "deviceState",
    "carState",
    "roadCameraState",
    "liveCalibration",
    "driverMonitoringState",
    "carControl",
    "liveDelay",
    "carrotMan",
    "radarState",
];

pub struct Options {
    pub binary: PathBuf,
    pub catalog: PathBuf,
    pub original_dir: PathBuf,
    pub original_manifest_sha256: String,
    pub collector: Option<PathBuf>,
    pub output: PathBuf,
    pub root: PathBuf,
    pub peer: Option<PathBuf>,
    pub compiler: Option<PathBuf>,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
type FileIdentity = (u64, u64, u64, i64, i64, i64, i64);
type CatalogFiles = BTreeMap<PathBuf, (FileIdentity, String)>;

fn identity(value: &Metadata) -> FileIdentity {
    (
        value.dev(),
        value.ino(),
        value.len(),
        value.mtime(),
        value.mtime_nsec(),
        value.ctime(),
        value.ctime_nsec(),
    )
}
struct Binding {
    path: PathBuf,
    file: File,
    metadata: Metadata,
    digest: String,
}
struct CatalogBinding {
    root: PathBuf,
    directory: File,
    identity: (u64, u64),
    files: CatalogFiles,
}
impl CatalogBinding {
    fn walk(root: &Path) -> Result<CatalogFiles> {
        fn visit(
            root: &Path,
            directory: &Path,
            files: &mut CatalogFiles,
            total: &mut u64,
        ) -> Result<()> {
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                let path = entry.path();
                let kind = entry.file_type()?;
                if kind.is_symlink() {
                    return Err("catalog contains symlink".into());
                }
                if kind.is_dir() {
                    visit(root, &path, files, total)?;
                    continue;
                }
                if !kind.is_file() {
                    return Err("catalog contains special file".into());
                }
                let fd = rustix::fs::open(
                    &path,
                    rustix::fs::OFlags::RDONLY
                        | rustix::fs::OFlags::NONBLOCK
                        | rustix::fs::OFlags::NOFOLLOW
                        | rustix::fs::OFlags::CLOEXEC,
                    rustix::fs::Mode::empty(),
                )?;
                let mut file = File::from(fd);
                let before = file.metadata()?;
                if !before.is_file() || before.len() > 512_000_000 {
                    return Err("catalog member not bounded regular file".into());
                }
                *total = total
                    .checked_add(before.len())
                    .ok_or("catalog length overflow")?;
                if *total > 2_000_000_000 || files.len() >= 2_000 {
                    return Err("catalog tree exceeds bound".into());
                }
                let mut hash = Sha256::new();
                let mut read = 0_u64;
                let mut chunk = [0_u8; 65_536];
                loop {
                    let n = file.read(&mut chunk)?;
                    if n == 0 {
                        break;
                    }
                    read += n as u64;
                    if read > before.len() {
                        return Err("catalog member grew during hashing".into());
                    }
                    hash.update(&chunk[..n]);
                }
                let after = file.metadata()?;
                if read != before.len() || identity(&before) != identity(&after) {
                    return Err("catalog member changed during hashing".into());
                }
                files.insert(
                    path.strip_prefix(root)?.to_owned(),
                    (identity(&before), format!("{:x}", hash.finalize())),
                );
            }
            Ok(())
        }
        let mut files = BTreeMap::new();
        let mut total = 0;
        visit(root, root, &mut files, &mut total)?;
        if !files.contains_key(Path::new("current.json")) || files.len() < 3 {
            return Err("catalog pointer or generation missing".into());
        }
        Ok(files)
    }
    fn open(root: PathBuf) -> Result<Self> {
        let fd = rustix::fs::open(
            &root,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        let directory = File::from(fd);
        let metadata = directory.metadata()?;
        if !metadata.is_dir() {
            return Err("trusted catalog is not a directory".into());
        }
        let files = Self::walk(&root)?;
        let binding = Self {
            root,
            directory,
            identity: (metadata.dev(), metadata.ino()),
            files,
        };
        binding.verify()?;
        Ok(binding)
    }
    fn verify(&self) -> Result<()> {
        let by_fd = self.directory.metadata()?;
        let by_path = fs::symlink_metadata(&self.root)?;
        if !by_path.is_dir()
            || by_path.file_type().is_symlink()
            || self.identity != (by_fd.dev(), by_fd.ino())
            || self.identity != (by_path.dev(), by_path.ino())
            || self.files != Self::walk(&self.root)?
        {
            return Err("trusted catalog tree changed".into());
        }
        Ok(())
    }
}
impl Binding {
    fn open(path: PathBuf, expected: Option<&str>, max: u64) -> Result<Self> {
        let fd = rustix::fs::open(
            &path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        let mut file = File::from(fd);
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() > max {
            return Err(format!("invalid driving source: {}", path.display()).into());
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(max + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != metadata.len() {
            return Err("driving source changed while binding".into());
        }
        let digest = digest(&bytes);
        if expected.is_some_and(|value| value != digest) {
            return Err(format!("driving source pin changed: {}", path.display()).into());
        }
        let binding = Self {
            path,
            file,
            metadata,
            digest,
        };
        binding.verify()?;
        Ok(binding)
    }
    fn verify(&self) -> Result<()> {
        if identity(&self.metadata) != identity(&self.file.metadata()?) {
            return Err(
                format!("driving source descriptor changed: {}", self.path.display()).into(),
            );
        }
        let current = Self::open_unchecked(&self.path, self.metadata.len())?;
        if identity(&self.metadata) != identity(&current.metadata) || self.digest != current.digest
        {
            return Err(format!("driving source path changed: {}", self.path.display()).into());
        }
        Ok(())
    }
    fn open_unchecked(path: &Path, max: u64) -> Result<Self> {
        let fd = rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        let mut file = File::from(fd);
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() > max {
            return Err("source no longer bounded regular file".into());
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(max + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != metadata.len() {
            return Err("source size drifted".into());
        }
        Ok(Self {
            path: path.into(),
            file,
            metadata,
            digest: digest(&bytes),
        })
    }
    fn require_host_elf(&self) -> Result<()> {
        let mut header = [0_u8; 20];
        if self.file.read_at(&mut header, 0)? != header.len()
            || &header[..6] != b"\x7fELF\x02\x01"
            || u16::from_le_bytes([header[18], header[19]])
                != if cfg!(target_arch = "aarch64") {
                    183
                } else {
                    62
                }
        {
            return Err(format!(
                "driving candidate is not a host ELF: {}",
                self.path.display()
            )
            .into());
        }
        Ok(())
    }
}

struct Signals {
    cancelled: Arc<AtomicBool>,
    registrations: Vec<signal_hook::SigId>,
}
impl Signals {
    fn new() -> Result<Self> {
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut registrations = Vec::new();
        for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            registrations.push(signal_hook::flag::register(signal, Arc::clone(&cancelled))?);
        }
        Ok(Self {
            cancelled,
            registrations,
        })
    }
    fn check(&self) -> Result<()> {
        if self.cancelled.load(Ordering::Relaxed) {
            Err("driving controller cancelled".into())
        } else {
            Ok(())
        }
    }
}
impl Drop for Signals {
    fn drop(&mut self) {
        for registration in self.registrations.drain(..) {
            signal_hook::low_level::unregister(registration);
        }
    }
}

fn now_ns() -> Result<u64> {
    let now = clock_gettime(ClockId::Monotonic);
    Ok(u64::try_from(now.tv_sec)? * 1_000_000_000 + u64::try_from(now.tv_nsec)?)
}

struct Lines {
    pipe: File,
    log: File,
    pending: Vec<u8>,
    total: usize,
}
impl Lines {
    fn until(&mut self, tokens: &[&str], signals: &Signals) -> Result<String> {
        let started = Instant::now();
        loop {
            signals.check()?;
            while let Some(index) = self.pending.iter().position(|byte| *byte == b'\n') {
                let line = self.pending.drain(..=index).collect::<Vec<_>>();
                let line = String::from_utf8(line)?.trim_end().to_owned();
                if tokens.iter().any(|token| line.contains(token)) {
                    return Ok(line);
                }
            }
            if started.elapsed() >= LINE_WAIT {
                return Err("driving child line wait exceeded 60 seconds".into());
            }
            let timeout = Timespec::try_from(Duration::from_millis(20))?;
            match poll(
                &mut [PollFd::new(&self.pipe, PollFlags::IN)],
                Some(&timeout),
            ) {
                Ok(_) => {}
                // Collector msgq publications notify this thread with SIGUSR2.
                // Recheck cancellation and the original deadline before waiting again.
                Err(rustix::io::Errno::INTR) => continue,
                Err(error) => return Err(error.into()),
            }
            let mut chunk = [0; 4096];
            match self.pipe.read(&mut chunk) {
                Ok(0) => return Err("driving child stream ended before required line".into()),
                Ok(n) => {
                    self.total = self
                        .total
                        .checked_add(n)
                        .ok_or("driving log length overflow")?;
                    if self.total > LOG_LIMIT {
                        return Err("driving child log exceeds 16 MiB".into());
                    }
                    self.log.write_all(&chunk[..n])?;
                    self.pending.extend_from_slice(&chunk[..n]);
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
}

struct OwnedChild {
    child: Child,
    lines: Lines,
    reaped: bool,
}
impl OwnedChild {
    fn spawn(
        program: &Path,
        args: &[OsString],
        env: &[(OsString, OsString)],
        log: &Path,
        peer: bool,
    ) -> Result<Self> {
        let mut command = Command::new(program);
        command
            .env_remove("SIMULATION")
            .env_remove("SEND_RAW_PRED")
            .env_remove("MODEL_LOG_FAULT")
            .env_remove("MODEL_LOG_FAULT_AUDIT")
            .args(args)
            .envs(env.iter().cloned())
            .process_group(0)
            .stdin(if peer { Stdio::piped() } else { Stdio::null() })
            .stdout(if peer { Stdio::piped() } else { Stdio::null() })
            .stderr(if peer {
                Stdio::from(File::create(log.with_extension("stderr"))?)
            } else {
                Stdio::piped()
            });
        let mut child = command.spawn()?;
        let pipe: OwnedFd = if peer {
            child.stdout.take().ok_or("peer stdout missing")?.into()
        } else {
            child.stderr.take().ok_or("daemon stderr missing")?.into()
        };
        let fd = rustix::fs::fcntl_getfl(&pipe)?;
        rustix::fs::fcntl_setfl(&pipe, fd | rustix::fs::OFlags::NONBLOCK)?;
        Ok(Self {
            child,
            lines: Lines {
                pipe: File::from(pipe),
                log: File::create(log)?,
                pending: Vec::new(),
                total: 0,
            },
            reaped: false,
        })
    }
    fn wait_line(&mut self, tokens: &[&str], signals: &Signals) -> Result<String> {
        self.lines.until(tokens, signals)
    }
    fn command(&mut self, value: &str, signals: &Signals) -> Result<String> {
        let input = self.child.stdin.as_mut().ok_or("peer stdin missing")?;
        input.write_all(value.as_bytes())?;
        input.write_all(b"\n")?;
        input.flush()?;
        self.wait_line(&["OK", "TIMEOUT"], signals)
    }
    fn alive(&mut self) -> Result<()> {
        if let Some(status) = self.child.try_wait()? {
            self.reaped = true;
            return Err(format!("driving daemon exited while waiting: {status}").into());
        }
        Ok(())
    }
    fn stop(&mut self, signal: Option<Signal>, signals: &Signals) -> Result<()> {
        signals.check()?;
        if let Some(signal) = signal {
            kill_process(Pid::from_child(&self.child), signal)?;
        }
        let started = Instant::now();
        loop {
            signals.check()?;
            if let Some(status) = self.child.try_wait()? {
                self.reaped = true;
                if status.success() {
                    return Ok(());
                }
                return Err(format!("driving child exited: {status}").into());
            }
            if started.elapsed() >= SHUTDOWN {
                return Err("driving child shutdown exceeded five seconds".into());
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = kill_process_group(Pid::from_child(&self.child), Signal::KILL);
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn checked_path(path: &Path) -> Result<&str> {
    let value = path.to_str().ok_or("peer protocol requires UTF-8 paths")?;
    if value.chars().any(char::is_whitespace) {
        return Err("peer protocol path contains whitespace".into());
    }
    Ok(value)
}
fn expected(response: String, required: &str) -> Result<()> {
    if response == required {
        Ok(())
    } else {
        Err(format!("peer replied {response}, expected {required}").into())
    }
}
fn write_params(directory: &Path) -> Result<()> {
    let mut message = capnp::message::Builder::new_default();
    message
        .init_root::<car_params::Builder<'_>>()
        .set_longitudinal_actuator_delay(0.0);
    let car = capnp::serialize::write_message_to_words(&message);
    let values: [(&str, &[u8]); 7] = [
        ("CarParams", car.as_slice()),
        ("VEgoStopping", b"5"),
        ("CameraYawTrimDeg", b"35"),
        ("UseWideCamera", b"1"),
        ("SteerActuatorDelay", b"0"),
        ("LatSmoothSec", b"0"),
        ("LongActuatorDelay", b"30"),
    ];
    for (name, bytes) in values {
        fs::write(directory.join(name), bytes)?;
    }
    Ok(())
}

fn build_peer(options: &Options, output: &Path) -> Result<PathBuf> {
    if let Some(peer) = &options.peer {
        return Ok(peer.clone());
    }
    let compiler = options
        .compiler
        .as_deref()
        .unwrap_or(Path::new("/usr/bin/g++"));
    let include = output.join("include");
    fs::create_dir(&include)?;
    fs::write(include.join("services.h"), services_header::header(true))?;
    let destination = output.join("driving-daemon-peer");
    let mut argv: Vec<OsString> = vec![
        compiler.as_os_str().to_owned(),
        OsStr::new("-std=c++17").to_owned(),
        OsStr::new("-pthread").to_owned(),
    ];
    argv.push(format!("-I{}", options.root.join("msgq_repo").display()).into());
    argv.push(format!("-I{}", include.display()).into());
    argv.extend(
        PEER_FILES
            .iter()
            .map(|name| options.root.join(name).into_os_string()),
    );
    argv.push("-o".into());
    argv.push(destination.as_os_str().to_owned());
    let mut spec = CommandSpec::new_os(argv, Duration::from_secs(180));
    spec.stdout_limit = LOG_LIMIT;
    spec.stderr_limit = LOG_LIMIT;
    let mut runner = NativeRunner;
    let result = runner.run_to_files(
        &spec,
        &output.join("peer-build.stdout"),
        &output.join("peer-build.stderr"),
    )?;
    if result.code != 0 {
        return Err(format!("peer compiler failed with {}", result.code).into());
    }
    Ok(destination)
}

fn checked_source_commit(root: &Path) -> Result<String> {
    let mut spec = CommandSpec::new_os(
        vec![
            "/usr/bin/git".into(),
            "-C".into(),
            root.as_os_str().to_owned(),
            "rev-parse".into(),
            "HEAD".into(),
        ],
        Duration::from_secs(10),
    );
    spec.stdout_limit = 128;
    spec.stderr_limit = 4096;
    let result = NativeRunner.run(&spec)?;
    if result.code != 0 {
        return Err("cannot identify driving source checkout".into());
    }
    let commit = String::from_utf8(result.stdout)?.trim().to_owned();
    if commit.len() != 40 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("driving source checkout does not have a full Git commit".into());
    }
    Ok(commit)
}

struct ScenarioContext<'a> {
    source_commit: Option<&'a str>,
    fault: Option<(&'a str, &'a Path)>,
}

fn scenario(
    options: &Options,
    original: &Original,
    case: &Value,
    peer_path: &Path,
    signals: &Signals,
    index: usize,
    context: ScenarioContext<'_>,
) -> Result<Value> {
    let width = case["resolution"][0].as_u64().ok_or("missing width")?;
    let height = case["resolution"][1].as_u64().ok_or("missing height")?;
    let mode = case["mode"].as_str().ok_or("missing stream mode")?;
    let layout = &case["image_layout"];
    let stride = layout["stride"].as_u64().ok_or("missing stride")?;
    let uv_offset = stride
        .checked_mul(layout["y_height"].as_u64().ok_or("missing y height")?)
        .ok_or("UV offset overflow")?;
    let size = layout["bytes"].as_u64().ok_or("missing frame bytes")?;
    let destination = options.output.join(format!("{width}-{mode}"));
    fs::create_dir(&destination)?;
    let prefix = format!("driving-qa-{}-{index}-{width}-{mode}", std::process::id());
    let shm = Path::new("/dev/shm").join(format!("msgq_{prefix}"));
    fs::create_dir(&shm)?;
    struct Shm(PathBuf);
    impl Drop for Shm {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _shm = Shm(shm);
    let params_root = destination.join("params");
    fs::create_dir(&params_root)?;
    let parameters = params_root.join(&prefix);
    // Match the original checker: establish the real namespace directory before
    // modeld starts, so Params does not create its own symlink at this path.
    fs::create_dir(&parameters)?;
    let mut env = vec![
        (OsString::from("OPENPILOT_PREFIX"), OsString::from(&prefix)),
        (
            OsString::from("PARAMS_ROOT"),
            params_root.as_os_str().to_owned(),
        ),
        (OsString::from("LOGPRINT"), OsString::from("info")),
    ];
    if mode == "dual" {
        env.push(("SEND_RAW_PRED".into(), "0".into()));
    }
    if let Some((mode, audit)) = context.fault {
        env.push(("MODEL_LOG_FAULT".into(), mode.into()));
        env.push(("MODEL_LOG_FAULT_AUDIT".into(), audit.as_os_str().to_owned()));
    }
    let mut collector = match (&options.collector, context.source_commit) {
        (Some(binary), Some(commit)) => Some(Collector::start(
            binary,
            &destination.join("logging"),
            &env,
            &prefix,
            &options.root,
            commit,
        )?),
        (None, None) => None,
        _ => return Err("driving collector source binding missing".into()),
    };
    let mut process_ids = Vec::new();
    let args = [
        OsString::from("--trusted-catalog"),
        options.catalog.as_os_str().to_owned(),
    ];
    let mut camera_wait = OwnedChild::spawn(
        &options.binary,
        &args,
        &env,
        &destination.join("camera-wait.log"),
        false,
    )?;
    process_ids.push(camera_wait.child.id());
    camera_wait.wait_line(&["modeld init"], signals)?;
    thread::sleep(Duration::from_millis(150));
    camera_wait.alive()?;
    camera_wait.stop(Some(Signal::INT), signals)?;
    let peer_args: Vec<OsString> = [width, height, stride, uv_offset, size]
        .into_iter()
        .map(|number| number.to_string().into())
        .chain(std::iter::once(mode.into()))
        .collect();
    let mut peer = OwnedChild::spawn(
        peer_path,
        &peer_args,
        &env,
        &destination.join("peer.log"),
        true,
    )?;
    peer.wait_line(&["READY"], signals)?;
    let warmup = original.read(
        case["warmup"]["image"]
            .as_str()
            .ok_or("warmup image missing")?,
    )?;
    if warmup.len() as u64 != size {
        return Err("warmup image size drift".into());
    }
    let warmup_path = destination.join("warmup.bin");
    fs::write(&warmup_path, warmup)?;
    let send = |peer: &mut OwnedChild, value: String| -> Result<()> {
        expected(peer.command(&value, signals)?, "OK")
    };
    let mut params_wait = OwnedChild::spawn(
        &options.binary,
        &args,
        &env,
        &destination.join("params-wait.log"),
        false,
    )?;
    process_ids.push(params_wait.child.id());
    params_wait.wait_line(
        &[if mode == "dual" {
            "connected extra cam"
        } else {
            "connected main cam"
        }],
        signals,
    )?;
    params_wait.wait_line(&["models loaded"], signals)?;
    send(
        &mut peer,
        format!("send 2 {0} {0}", checked_path(&warmup_path)?),
    )?;
    thread::sleep(Duration::from_millis(150));
    params_wait.alive()?;
    params_wait.stop(Some(Signal::TERM), signals)?;
    write_params(&parameters)?;
    if mode == "road" {
        let mut first_wait = OwnedChild::spawn(
            &options.binary,
            &args,
            &env,
            &destination.join("frame-wait.log"),
            false,
        )?;
        process_ids.push(first_wait.child.id());
        first_wait.wait_line(&["modeld got CarParams"], signals)?;
        first_wait.stop(Some(Signal::TERM), signals)?;
    }
    let frames = case["frames"].as_array().ok_or("missing frames")?;
    let mut run_args = args.to_vec();
    run_args.extend([
        OsString::from("--frames"),
        OsString::from((frames.len() + 1).to_string()),
    ]);
    let mut daemon = OwnedChild::spawn(
        &options.binary,
        &run_args,
        &env,
        &destination.join("daemon.log"),
        false,
    )?;
    process_ids.push(daemon.child.id());
    let daemon_pid = daemon.child.id();
    daemon.wait_line(
        &[if mode == "dual" {
            "connected extra cam"
        } else {
            "connected main cam"
        }],
        signals,
    )?;
    daemon.wait_line(&["models loaded"], signals)?;
    daemon.wait_line(&["modeld got CarParams"], signals)?;
    send(
        &mut peer,
        format!("send 2 {0} {0}", checked_path(&warmup_path)?),
    )?;
    let absent = destination.join("unexpected.bin");
    for topic in OUTPUTS {
        expected(
            peer.command(
                &format!("receive {topic} 200 {}", checked_path(&absent)?),
                signals,
            )?,
            "TIMEOUT",
        )?;
    }
    let mut fields = 0_usize;
    let mut messages = 0_usize;
    let mut trace = Vec::with_capacity(frames.len());
    for frame in frames {
        signals.check()?;
        let frame_id = frame["frame_id"].as_u64().ok_or("frame id missing")?;
        let topics = frame["topics"].as_object().ok_or("input topics missing")?;
        for topic in TOPICS {
            let Some(name) = topics.get(topic) else {
                if topic == "liveCalibration" {
                    continue;
                }
                return Err(format!("missing original input topic {topic}").into());
            };
            let path = destination.join(format!("{topic}-{frame_id}.bin"));
            fs::write(
                &path,
                original.read(name.as_str().ok_or("input member missing")?)?,
            )?;
            send(
                &mut peer,
                format!("publish {topic} {}", checked_path(&path)?),
            )?;
        }
        let images = &frame["images"];
        let main = destination.join(format!("main-{frame_id}.bin"));
        let extra = destination.join(format!("extra-{frame_id}.bin"));
        for (stream, path) in [("main", &main), ("extra", &extra)] {
            let bytes = original.read(images[stream].as_str().ok_or("image member missing")?)?;
            if bytes.len() as u64 != size {
                return Err("frame image size drift".into());
            }
            fs::write(path, bytes)?;
        }
        let start = now_ns()?;
        send(
            &mut peer,
            format!(
                "send {frame_id} {} {}",
                checked_path(&main)?,
                checked_path(&extra)?
            ),
        )?;
        let prepare_only = frame["prepare_only"]
            .as_bool()
            .ok_or("prepare-only missing")?;
        let mut elapsed = None;
        for topic in OUTPUTS {
            let output = destination.join(format!("{topic}-output-{frame_id}.bin"));
            let timeout = if prepare_only { 200 } else { 30_000 };
            let response = peer.command(
                &format!("receive {topic} {timeout} {}", checked_path(&output)?),
                signals,
            )?;
            if prepare_only {
                expected(response, "TIMEOUT")?;
                if output.exists() {
                    return Err("prepare-only frame produced packet".into());
                }
                continue;
            }
            expected(response, "OK")?;
            let packet = fs::read(&output)?;
            if packet.len() > 4_000_000 {
                return Err("publication packet exceeds 4 MiB".into());
            }
            let publication = &frame["publications"][topic];
            let expected: Value = serde_json::from_slice(
                &original.read(
                    publication["decoded"]
                        .as_str()
                        .ok_or("decoded reference missing")?,
                )?,
            )?;
            let raw = if topic == "modelV2" && mode == "dual" {
                Some(
                    original.read(
                        frame["raw_prediction"]
                            .as_str()
                            .ok_or("raw prediction missing")?,
                    )?,
                )
            } else {
                None
            };
            let result =
                compare_packet(&expected, &packet, topic, start, now_ns()?, raw.as_deref())?;
            if topic == "modelV2" {
                elapsed = Some(result.model_execution_time);
            }
            if topic == "drivingModelData" && elapsed != Some(result.model_execution_time) {
                return Err("model execution times disagree".into());
            }
            fields += result.fields;
            messages += 1;
        }
        trace.push(json!({"frame_id": frame_id, "prepare_only": prepare_only,
            "desire": frame["desire"], "is_rhd": frame["is_rhd"], "packed_sha256": frame["packed_sha256"]}));
        for topic in OUTPUTS {
            expected(
                peer.command(
                    &format!("receive {topic} 100 {}", checked_path(&absent)?),
                    signals,
                )?,
                "TIMEOUT",
            )?;
        }
    }
    daemon.stop(None, signals)?;
    send(&mut peer, "stop".into())?;
    peer.stop(None, signals)?;
    let logging = if let Some(collector) = &mut collector {
        let mut frame_ids = Vec::with_capacity(trace.len() + 1);
        frame_ids.push(2);
        for step in &trace {
            frame_ids.push(step["frame_id"].as_u64().ok_or("trace frame ID missing")?);
        }
        let source = DrivingScenario {
            pid: daemon_pid,
            width,
            height,
            size,
            mode,
            frames: &frame_ids,
        };
        Some(collector.finish(&process_ids, &source)?)
    } else {
        None
    };
    let catalog = services_header::catalog();
    let services = catalog
        .as_array()
        .ok_or("Rust services catalog is not an array")?;
    let mut queue_capacities = serde_json::Map::new();
    for topic in TOPICS.into_iter().chain(OUTPUTS) {
        let row = services
            .iter()
            .find(|row| row["name"] == topic)
            .ok_or("missing queue service")?;
        queue_capacities.insert(
            topic.into(),
            json!(row["queue_size"].as_u64().ok_or("missing queue capacity")?),
        );
    }
    Ok(
        json!({"resolution": [width, height], "streams": mode, "frames": frames.len(),
        "messages": messages, "compared_fields": fields, "raw_predictions": mode == "dual",
        "raw_comparison": "exact bytes", "float_tolerance": 1e-6,
        "polynomial_tolerance": 2e-5, "discrete_fields": "exact", "device_validation": false,
        "trace": trace, "queue_capacities": queue_capacities,
        "llvm_path": std::env::var("LLVM_PATH").ok(), "logging": logging,
        "startup": {"model_loaded_before_car_params": true,
            "model_loaded_before_first_frame": true, "first_frame_id": 2,
            "first_frame_prepare_only": true},
        "signal_checks": if mode == "road" { vec!["SIGINT camera discovery", "SIGTERM CarParams wait", "SIGTERM first-frame wait"] }
                         else { vec!["SIGINT camera discovery", "SIGTERM CarParams wait"] }}),
    )
}

struct Completed {
    report: Value,
    original: Original,
    catalog: CatalogBinding,
    bindings: Vec<Binding>,
    signals: Signals,
    output: PathBuf,
    output_dir: File,
    output_identity: (u64, u64),
    receipt: File,
    receipt_identity: (u64, u64, u64, i64, i64, i64, i64),
    receipt_bytes: Vec<u8>,
}
impl Completed {
    fn verify(&self) -> Result<()> {
        self.signals.check()?;
        self.original.verify()?;
        self.catalog.verify()?;
        for binding in &self.bindings {
            binding.verify()?;
        }
        let directory = self.output_dir.metadata()?;
        let path = fs::symlink_metadata(&self.output)?;
        if !path.is_dir()
            || path.file_type().is_symlink()
            || self.output_identity != (directory.dev(), directory.ino())
            || self.output_identity != (path.dev(), path.ino())
        {
            return Err("driving output directory replaced".into());
        }
        let receipt = fs::symlink_metadata(self.output.join("report.json"))?;
        if !receipt.is_file()
            || receipt.file_type().is_symlink()
            || identity(&receipt) != self.receipt_identity
            || identity(&self.receipt.metadata()?) != self.receipt_identity
            || fs::read(self.output.join("report.json"))? != self.receipt_bytes
        {
            return Err("driving report receipt changed".into());
        }
        Ok(())
    }
}

/// Own the output directory and all children. The receipt is written only after
/// every six-scenario IPC comparison and a final input/source verification.
fn execute(options: &Options, only_dual: bool, fault: Option<(&str, &Path)>) -> Result<Completed> {
    let signals = Signals::new()?;
    let original = Original::open(&options.original_dir, &options.original_manifest_sha256)?;
    let source_pins = original.manifest()["source_pins"]
        .as_object()
        .ok_or("original source pins missing")?;
    let mut bindings = Vec::new();
    for (relative, pin) in source_pins {
        bindings.push(Binding::open(
            options.root.join(relative),
            pin.as_str(),
            4_000_000,
        )?);
    }
    for relative in PEER_FILES {
        bindings.push(Binding::open(options.root.join(relative), None, 4_000_000)?);
    }
    let binary = Binding::open(options.binary.clone(), None, 512_000_000)?;
    binary.require_host_elf()?;
    bindings.push(binary);
    let source_commit = if let Some(collector) = &options.collector {
        let collector = Binding::open(collector.clone(), None, 512_000_000)?;
        collector.require_host_elf()?;
        bindings.push(collector);
        Some(checked_source_commit(&options.root)?)
    } else {
        None
    };
    let catalog = CatalogBinding::open(options.catalog.clone())?;
    model_source::verify(&options.catalog, original.manifest())?;
    fs::create_dir(&options.output)?;
    if let Some((_, audit)) = fault {
        if audit != options.output.join("audit") {
            return Err("fault audit path must belong to owned output".into());
        }
        fs::create_dir(audit)?;
    }
    let output_dir = File::from(rustix::fs::open(
        &options.output,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::DIRECTORY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?);
    let output_metadata = output_dir.metadata()?;
    let peer = build_peer(options, &options.output)?;
    let peer_binary = Binding::open(peer.clone(), None, 128_000_000)?;
    peer_binary.require_host_elf()?;
    bindings.push(peer_binary);
    let mut reports = Vec::new();
    for (index, case) in original.scenarios().iter().enumerate() {
        if only_dual && (case["resolution"][0] != 1344 || case["mode"] != "dual") {
            continue;
        }
        signals.check()?;
        reports.push(scenario(
            options,
            &original,
            case,
            &peer,
            &signals,
            index,
            ScenarioContext {
                source_commit: source_commit.as_deref(),
                fault,
            },
        )?);
    }
    if reports.len() != if only_dual { 1 } else { 6 }
        || reports
            .iter()
            .map(|row| row["messages"].as_u64().unwrap_or(0))
            .sum::<u64>()
            != if only_dual { 33 } else { 114 }
    {
        return Err("driving scenario publication count changed".into());
    }
    original.verify()?;
    catalog.verify()?;
    for binding in &bindings {
        binding.verify()?;
    }
    let output_now = fs::symlink_metadata(&options.output)?;
    if !output_now.is_dir()
        || (output_now.dev(), output_now.ino()) != (output_metadata.dev(), output_metadata.ino())
    {
        return Err("driving output directory replaced".into());
    }
    let report = json!({"runs": reports, "device_validation": false});
    let mut receipt_bytes = serde_json::to_vec_pretty(&report)?;
    receipt_bytes.push(b'\n');
    let mut receipt = File::from(rustix::fs::openat(
        &output_dir,
        "report.json",
        rustix::fs::OFlags::RDWR
            | rustix::fs::OFlags::CREATE
            | rustix::fs::OFlags::EXCL
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )?);
    receipt.write_all(&receipt_bytes)?;
    receipt.sync_all()?;
    let receipt_identity = identity(&receipt.metadata()?);
    let completed = Completed {
        report,
        original,
        catalog,
        bindings,
        signals,
        output: options.output.clone(),
        output_dir,
        output_identity: (output_metadata.dev(), output_metadata.ino()),
        receipt,
        receipt_identity,
        receipt_bytes,
    };
    completed.verify()?;
    Ok(completed)
}

pub fn run(options: &Options) -> Result<Value> {
    let completed = execute(options, false, None)?;
    completed.verify()?;
    Ok(completed.report.clone())
}

/// The link-fault checker reuses the exact live 1344-dual scenario, including
/// the same packet comparator and collector. The ordinary entry point still
/// requires all six scenarios and all 114 published packets.
pub fn run_only_dual(options: &Options) -> Result<Value> {
    let completed = execute(options, true, None)?;
    completed.verify()?;
    Ok(completed.report.clone())
}

pub fn run_link_fault_dual(options: &Options, mode: &str, audit: &Path) -> Result<Value> {
    if !matches!(mode, "timing-error" | "timing-full") || options.collector.is_none() {
        return Err("link fault requires a known timing mode and live collector".into());
    }
    let completed = execute(options, true, Some((mode, audit)))?;
    completed.verify()?;
    Ok(completed.report.clone())
}

/// Leaf CLI. Parent registration should expose this through an explicitly
/// feature-gated bin; the original checker remains in place until cloud proof.
pub fn cli() -> Result<()> {
    let mut binary = None;
    let mut catalog = None;
    let mut original_dir = None;
    let mut original_manifest_sha256 = None;
    let mut collector = None;
    let mut output = None;
    let mut root = None;
    let mut peer = None;
    let mut compiler = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(option) = args.next() {
        let value = args
            .next()
            .ok_or("driving controller option requires a value")?;
        let slot = match option.to_str() {
            Some("--binary") => &mut binary,
            Some("--catalog") => &mut catalog,
            Some("--original-dir") => &mut original_dir,
            Some("--original-manifest-sha256") => &mut original_manifest_sha256,
            Some("--collector") => &mut collector,
            Some("--output") => &mut output,
            Some("--root") => &mut root,
            Some("--peer") => &mut peer,
            Some("--compiler") => &mut compiler,
            _ => return Err("unknown driving controller option".into()),
        };
        if slot.replace(value).is_some() {
            return Err("duplicate driving controller option".into());
        }
    }
    let options = Options {
        binary: PathBuf::from(binary.ok_or("--binary required")?),
        catalog: PathBuf::from(catalog.ok_or("--catalog required")?),
        original_dir: PathBuf::from(original_dir.ok_or("--original-dir required")?),
        original_manifest_sha256: original_manifest_sha256
            .ok_or("--original-manifest-sha256 required")?
            .into_string()
            .map_err(|_| "manifest SHA must be UTF-8")?,
        collector: collector.map(PathBuf::from),
        output: PathBuf::from(output.ok_or("--output required")?),
        root: root.map(PathBuf::from).unwrap_or(std::env::current_dir()?),
        peer: peer.map(PathBuf::from),
        compiler: compiler.map(PathBuf::from),
    };
    let completed = execute(&options, false, None)?;
    println!("{}", serde_json::to_string(&completed.report)?);
    completed.verify()?;
    Ok(())
}

#[cfg(test)]
mod line_tests {
    use super::{Lines, Signals};
    use std::{
        fs::File,
        io::Write,
        os::{fd::OwnedFd, unix::net::UnixStream},
    };

    #[test]
    fn buffered_later_match_survives_stream_eof() {
        let directory = tempfile::tempdir().unwrap();
        let (mut writer, reader) = UnixStream::pair().unwrap();
        writer.write_all(b"unrelated\nREADY\n").unwrap();
        drop(writer);
        let descriptor: OwnedFd = reader.into();
        let mut lines = Lines {
            pipe: File::from(descriptor),
            log: File::create(directory.path().join("peer.log")).unwrap(),
            pending: Vec::new(),
            total: 0,
        };
        let signals = Signals::new().unwrap();
        assert_eq!(lines.until(&["READY"], &signals).unwrap(), "READY");
    }

    #[test]
    fn msgq_notification_signals_do_not_abort_a_child_line_wait() {
        use openpilot_msgq::{Publisher, Subscriber};
        use std::{sync::mpsc, thread, time::Duration};
        const NAME: &str = "driving_daemon_check::line_tests::msgq_notification_signals_do_not_abort_a_child_line_wait";
        const CHILD: &str = "OPENPILOT_DRIVING_LINE_SIGNAL_CHILD";
        // Keep the process-wide msgq handler and namespace isolated from other
        // lib tests. The parent owns the shared-memory files until the child exits.
        if std::env::var_os(CHILD).is_none() {
            let namespace = tempfile::Builder::new()
                .prefix("msgq_rust-probe-line-eintr-")
                .tempdir_in("/dev/shm")
                .unwrap();
            let prefix = namespace
                .path()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .strip_prefix("msgq_")
                .unwrap();
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--nocapture"])
                .env(CHILD, "1")
                .env("OPENPILOT_PREFIX", prefix)
                .env_remove("CEREAL_FAKE")
                .status()
                .unwrap();
            assert!(status.success(), "msgq signal child failed: {status}");
            return;
        }
        let mut publisher = Publisher::new("line-interruption").unwrap();
        let directory = tempfile::tempdir().unwrap();
        let (mut writer, reader) = UnixStream::pair().unwrap();
        reader.set_nonblocking(true).unwrap();
        let descriptor: OwnedFd = reader.into();
        let log = File::create(directory.path().join("signal.log")).unwrap();
        let (ready, started) = mpsc::channel();
        let waiting = thread::spawn(move || {
            // Queue registration records this thread ID. Each publication below
            // sends this reader the real msgq SIGUSR2 notification.
            let mut subscriber = Subscriber::new("line-interruption", false).unwrap();
            let mut lines = Lines {
                pipe: File::from(descriptor),
                log,
                pending: Vec::new(),
                total: 0,
            };
            let signals = Signals::new().unwrap();
            ready.send(()).unwrap();
            let line = lines
                .until(&["READY"], &signals)
                .map_err(|error| error.to_string())?;
            for value in 0_u8..5 {
                assert_eq!(
                    subscriber.receive(Duration::ZERO).unwrap(),
                    Some(vec![value])
                );
            }
            Ok::<_, String>(line)
        });
        started.recv().unwrap();
        for value in 0_u8..5 {
            thread::sleep(Duration::from_millis(10));
            publisher.send(&[value]).unwrap();
        }
        let written = writer.write_all(b"READY\n");
        // Report the original wait error before a secondary closed-pipe error.
        assert_eq!(waiting.join().unwrap().unwrap(), "READY");
        written.unwrap();
    }
}
