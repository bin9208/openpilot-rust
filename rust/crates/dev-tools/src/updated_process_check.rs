//! Updater command validation against pinned original-source observations.
//! Reference rows are explicitly marked as frozen baseline evidence; candidate
//! process, signal, group-lifetime and timeout checks execute on every run.
use crate::{
    ci_policy,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
    updated_process_fixture,
};
use rustix::{
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
    process::{kill_process, kill_process_group, waitid, Pid, Signal, WaitId, WaitIdOptions},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Read, Write},
    os::{
        fd::OwnedFd,
        unix::process::{CommandExt, ExitStatusExt},
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
const BASELINE_SHA256: &str = "b7a2ed108a17d2010516112b20e491493aace5c26d43bf4d84a135bacc2be752";
pub fn load_baseline(path: &Path) -> Result<Value> {
    let bytes = fs::read(path)?;
    if format!("{:x}", Sha256::digest(&bytes)) != BASELINE_SHA256 {
        return Err("missing, modified or stale updated-process source baseline".into());
    }
    let value: Value = serde_json::from_slice(&bytes)?;
    if value["fixture"]["source_sha256"] != updated_process_fixture::source_sha256() {
        return Err("stale updated-process baseline: native fixture source changed".into());
    }
    Ok(value)
}
pub fn alive_from(read: impl FnOnce() -> io::Result<String>) -> io::Result<bool> {
    ci_policy::process_alive_from(read)
}
fn alive(pid: i32) -> io::Result<bool> {
    alive_from(|| fs::read_to_string(format!("/proc/{pid}/stat")))
}
fn require(condition: bool, message: impl Into<String>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into().into())
    }
}
struct OwnedChild {
    child: Child,
    pid: Pid,
    reaped: bool,
}
impl OwnedChild {
    fn finish(&mut self) -> Result<std::process::ExitStatus> {
        match kill_process_group(self.pid, Signal::KILL) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => {}
            Err(error) => return Err(error.into()),
        }
        let status = self.child.wait()?;
        self.reaped = true;
        Ok(status)
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.finish();
        }
    }
}
struct Stream {
    file: File,
    bytes: Vec<u8>,
    eof: bool,
}
impl Stream {
    fn new(fd: OwnedFd) -> Result<Self> {
        let file = File::from(fd);
        fcntl_setfl(&file, fcntl_getfl(&file)? | OFlags::NONBLOCK)?;
        Ok(Self {
            file,
            bytes: vec![],
            eof: false,
        })
    }
    fn read(&mut self) -> Result<()> {
        if self.eof {
            return Ok(());
        }
        let mut buffer = [0; 16384];
        match self.file.read(&mut buffer) {
            Ok(0) => self.eof = true,
            Ok(n) => {
                require(
                    self.bytes.len() + n <= 16 * 1024 * 1024,
                    "updated-process probe output limit exceeded",
                )?;
                self.bytes.extend_from_slice(&buffer[..n]);
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) => {}
            Err(e) => return Err(e.into()),
        };
        Ok(())
    }
}
struct Probe {
    owned: OwnedChild,
    stdout: Stream,
    stderr: Stream,
}
struct Output {
    code: i32,
    stdout: String,
    stderr: String,
}
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
impl Probe {
    fn spawn(
        binary: &Path,
        config: &Value,
        environment: &BTreeMap<String, String>,
    ) -> Result<Self> {
        let child = Command::new(binary)
            .envs(environment)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()?;
        let pid = Pid::from_raw(i32::try_from(child.id())?).ok_or("invalid updater probe pid")?;
        let mut owned = OwnedChild {
            child,
            pid,
            reaped: false,
        };
        let stdout = Stream::new(
            owned
                .child
                .stdout
                .take()
                .ok_or("missing updater stdout")?
                .into(),
        )?;
        let stderr = Stream::new(
            owned
                .child
                .stderr
                .take()
                .ok_or("missing updater stderr")?
                .into(),
        )?;
        let mut input = owned.child.stdin.take().ok_or("missing updater stdin")?;
        input.write_all(&serde_json::to_vec(config)?)?;
        drop(input);
        Ok(Self {
            owned,
            stdout,
            stderr,
        })
    }
    fn capture(mut self, timeout: Duration) -> Result<Output> {
        let deadline = Instant::now() + timeout;
        let mut exited = false;
        loop {
            self.stdout.read()?;
            self.stderr.read()?;
            if !exited {
                match waitid(
                    WaitId::Pid(self.owned.pid),
                    WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
                ) {
                    Ok(Some(_)) => exited = true,
                    Ok(None) | Err(rustix::io::Errno::INTR) => {}
                    Err(error) => return Err(error.into()),
                }
            }
            if exited && self.stdout.eof && self.stderr.eof {
                let status = self.owned.finish()?;
                return Ok(Output {
                    code: status
                        .code()
                        .unwrap_or_else(|| -status.signal().unwrap_or(1)),
                    stdout: text(std::mem::take(&mut self.stdout.bytes))?,
                    stderr: text(std::mem::take(&mut self.stderr.bytes))?,
                });
            }
            require(Instant::now() < deadline, "updated-process probe timed out")?;
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn interrupt(&self) -> Result<()> {
        kill_process(self.owned.pid, Signal::INT)?;
        Ok(())
    }
}
#[derive(Debug, PartialEq, Eq)]
struct ProcessIdentity {
    parent: i32,
    group: i32,
    session: i32,
    started: u64,
}
fn process_identity(pid: i32) -> Result<ProcessIdentity> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let fields = stat
        .rsplit_once(") ")
        .ok_or("invalid fixture process stat")?
        .1
        .split_whitespace()
        .collect::<Vec<_>>();
    require(fields.len() >= 20, "truncated fixture process stat")?;
    Ok(ProcessIdentity {
        parent: fields[1].parse()?,
        group: fields[2].parse()?,
        session: fields[3].parse()?,
        started: fields[19].parse()?,
    })
}
/// The source-pinned fixture has one worker and no children. Its pidfd remains
/// a safe cleanup identity even after the runtime reaps the separate group
/// leader. An old numeric group report grants no later signal permission.
struct WorkerGuard {
    fd: OwnedFd,
}
impl WorkerGuard {
    fn stopped(&self, timeout: Duration) -> Result<bool> {
        use rustix::event::{poll, PollFd, PollFlags, Timespec};
        let began = Instant::now();
        loop {
            let remaining = timeout.saturating_sub(began.elapsed());
            let timespec = Timespec {
                tv_sec: i64::try_from(remaining.as_secs())?,
                tv_nsec: i64::from(remaining.subsec_nanos()),
            };
            match poll(&mut [PollFd::new(&self.fd, PollFlags::IN)], Some(&timespec)) {
                Ok(count) => return Ok(count > 0),
                Err(rustix::io::Errno::INTR) if began.elapsed() < timeout => {}
                Err(rustix::io::Errno::INTR) => return Ok(false),
                Err(error) => return Err(error.into()),
            }
        }
    }
    fn stop(&self) -> Result<()> {
        if self.stopped(Duration::ZERO)? {
            return Ok(());
        }
        match rustix::process::pidfd_send_signal(&self.fd, Signal::KILL) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => {}
            Err(error) => return Err(error.into()),
        }
        require(
            self.stopped(Duration::from_secs(1))?,
            "owned fixture worker survived cleanup",
        )
    }
}
impl Drop for WorkerGuard {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
fn pin_worker(worker: i32, group: Pid, owner: Pid) -> Result<WorkerGuard> {
    let group = group.as_raw_nonzero().get();
    let leader_before = process_identity(group)?;
    require(
        leader_before.parent == owner.as_raw_nonzero().get() && leader_before.group == group,
        "reported fixture group is not a direct child of the owned probe",
    )?;
    let worker_before = process_identity(worker)?;
    require(
        worker_before.group == group && worker_before.session == leader_before.session,
        "reported fixture worker does not belong to the owned group",
    )?;
    require(alive(worker)?, "reported fixture worker already exited")?;
    let pid = Pid::from_raw(worker).ok_or("invalid fixture worker pid")?;
    let fd = rustix::process::pidfd_open(pid, rustix::process::PidfdFlags::empty())?;
    require(
        process_identity(worker)? == worker_before && process_identity(group)? == leader_before,
        "fixture process identity changed while acquiring pidfd",
    )?;
    Ok(WorkerGuard { fd })
}
fn worker_ready(path: &Path, owner: Pid) -> Result<(i32, WorkerGuard)> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match fs::read_to_string(path) {
            Ok(value) if !value.is_empty() => {
                let worker = value.parse::<i32>()?;
                let group =
                    fs::read_to_string(format!("{}.group", path.display()))?.parse::<i32>()?;
                let group = Pid::from_raw(group).ok_or("invalid fixture group")?;
                require(
                    group != rustix::process::getpgrp(),
                    "fixture must not share checker process group",
                )?;
                return Ok((worker, pin_worker(worker, group, owner)?));
            }
            Ok(_) => {}
            Err(e) if matches!(e.kind(), io::ErrorKind::NotFound) => {}
            Err(e) => return Err(e.into()),
        }
        require(
            Instant::now() < deadline,
            "updated command worker readiness timed out",
        )?;
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn require_stopped(worker: &WorkerGuard) -> Result<()> {
    require(
        worker.stopped(Duration::from_secs(1))?,
        "orphan survived updater interrupt",
    )
}
fn log(path: &Path, text: &str) -> Result<()> {
    fs::write(path, if text.is_empty() { "<empty>\n" } else { text })?;
    Ok(())
}
fn write_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
struct Options {
    binary: PathBuf,
    launcher: PathBuf,
    fixture: PathBuf,
    baseline: PathBuf,
    output: PathBuf,
}
impl Options {
    fn parse(arguments: &[String]) -> Result<Self> {
        let mut values = BTreeMap::new();
        let mut i = 0;
        while i < arguments.len() {
            let key = &arguments[i];
            require(
                [
                    "--binary",
                    "--launcher",
                    "--fixture",
                    "--baseline",
                    "--output",
                ]
                .contains(&key.as_str()),
                format!("unknown updated-process option {key}"),
            )?;
            i += 1;
            let value = arguments
                .get(i)
                .ok_or_else(|| format!("{key} requires a value"))?;
            require(
                values.insert(key.clone(), PathBuf::from(value)).is_none(),
                format!("duplicate updated-process option {key}"),
            )?;
            i += 1;
        }
        let mut take = |name: &str| -> Result<PathBuf> {
            values
                .remove(name)
                .ok_or_else(|| format!("{name} required").into())
        };
        Ok(Self {
            binary: take("--binary")?,
            launcher: take("--launcher")?,
            fixture: take("--fixture")?,
            baseline: take("--baseline")?,
            output: take("--output")?,
        })
    }
}
fn environment() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
        ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
        ("GIT_CONFIG_COUNT".into(), "2".into()),
        ("GIT_CONFIG_KEY_0".into(), "test.preserved".into()),
        ("GIT_CONFIG_VALUE_0".into(), "keep".into()),
        ("GIT_CONFIG_KEY_1".into(), "gc.auto".into()),
        ("GIT_CONFIG_VALUE_1".into(), "1".into()),
    ])
}
fn config(launcher: &Path, argv: Vec<String>, root: &Path) -> Value {
    json!({"launcher":launcher,"argv":argv,"cwd":root,"endpoint":format!("ipc://{}",root.join("logs.sock").display())})
}
pub fn run(arguments: &[String]) -> Result<Value> {
    let options = Options::parse(arguments)?;
    // Validate before creating output or executing even the fixture identity probe.
    let baseline = load_baseline(&options.baseline)?;
    let binary = options.binary.canonicalize()?;
    let launcher = options.launcher.canonicalize()?;
    let fixture = options.fixture.canonicalize()?;
    let identity = NativeRunner.run(&CommandSpec::new(
        vec![fixture.display().to_string(), "identity".into()],
        Duration::from_secs(5),
    ))?;
    require(
        identity.code == 0,
        "updated-process fixture identity failed",
    )?;
    require(
        serde_json::from_slice::<Value>(&identity.stdout)? == baseline["fixture"],
        "running updated-process fixture does not match baseline source identity",
    )?;
    fs::create_dir_all(&options.output)?;
    write_json(&options.output.join("source-baseline.json"), &baseline)?;
    let temp = tempfile::Builder::new()
        .prefix("updated-command-")
        .tempdir()?;
    let root = temp.path();
    let environment = environment();
    let mut summary = vec![];
    for case in baseline["normal"]
        .as_array()
        .ok_or("missing original normal cases")?
    {
        let name = case["name"].as_str().ok_or("missing case name")?;
        let value = config(
            &launcher,
            vec![fixture.display().to_string(), name.into()],
            root,
        );
        let output =
            Probe::spawn(&binary, &value, &environment)?.capture(Duration::from_secs(10))?;
        log(
            &options.output.join(format!("{name}-native.stdout")),
            &output.stdout,
        )?;
        log(
            &options.output.join(format!("{name}-native.stderr")),
            &output.stderr,
        )?;
        log(
            &options.output.join(format!("{name}-source.stdout")),
            case["stdout"].as_str().ok_or("missing original stdout")?,
        )?;
        log(
            &options.output.join(format!("{name}-source.stderr")),
            case["stderr"].as_str().ok_or("missing original stderr")?,
        )?;
        require(
            output.code == 0,
            format!(
                "updated command {name} exited {}: {}",
                output.code, output.stderr
            ),
        )?;
        let actual: Value = serde_json::from_str(&output.stdout)?;
        require(
            actual == case["result"],
            format!("updated command mismatch for {name}: {actual}"),
        )?;
        summary.push(json!({"scenario":name,"passed":true,"result":actual,"reference":"pinned-original-source-baseline"}));
    }
    for case in baseline["groups"]
        .as_array()
        .ok_or("missing original group cases")?
    {
        let parent_exits = case["parent_exits"]
            .as_bool()
            .ok_or("missing parent lifetime")?;
        let label = if parent_exits { "True" } else { "False" };
        let source_prefix = format!("group-source-{label}");
        log(
            &options.output.join(format!("{source_prefix}.stdout")),
            case["stdout"]
                .as_str()
                .ok_or("missing original group stdout")?,
        )?;
        log(
            &options.output.join(format!("{source_prefix}.stderr")),
            case["stderr"]
                .as_str()
                .ok_or("missing original group stderr")?,
        )?;
        summary.push(json!({"scenario":source_prefix,"passed":true,"elapsed":case["elapsed"],"worker_stopped":true,"exit_code":0,"baseline":true}));
        let pidfile = root.join(format!("native-{label}.pid"));
        let value = config(
            &launcher,
            vec![
                fixture.display().to_string(),
                "group-leader".into(),
                pidfile.display().to_string(),
                parent_exits.to_string(),
            ],
            root,
        );
        let probe = Probe::spawn(&binary, &value, &environment)?;
        let (worker, worker_guard) = match worker_ready(&pidfile, probe.owned.pid) {
            Ok(ready) => ready,
            Err(error) => {
                // The report may be invalid, so do not signal anything named in
                // it. Ask only our still-owned probe to close its nested command
                // through the existing interrupt/capture path, preserving the
                // primary report error and the established four-second budget.
                let _ = probe.interrupt();
                let _ = probe.capture(Duration::from_secs(4));
                return Err(error);
            }
        };
        require(alive(worker)?, "fixture worker stopped before interrupt")?;
        let started = Instant::now();
        probe.interrupt()?;
        let output = probe.capture(Duration::from_secs(4))?;
        let elapsed = started.elapsed().as_secs_f64();
        require(
            output.code == 0,
            format!("updated interrupt failed: {}", output.stderr),
        )?;
        require(
            serde_json::from_str::<Value>(&output.stdout)? == case["result"],
            "updated interrupt did not report interruption",
        )?;
        require_stopped(&worker_guard)?;
        let prefix = format!("group-native-{label}");
        log(
            &options.output.join(format!("{prefix}.stdout")),
            &output.stdout,
        )?;
        log(
            &options.output.join(format!("{prefix}.stderr")),
            &output.stderr,
        )?;
        summary.push(json!({"scenario":prefix,"passed":true,"elapsed":elapsed,"worker_stopped":true,"exit_code":output.code,"baseline":false}));
    }
    let report = json!({"passed":true,"cases":summary,"source_baseline":{"sha256":BASELINE_SHA256,"source_commit":baseline["source_commit"],"source_sha256":baseline["source_sha256"],"source_rows_are_frozen":true},"fixture":baseline["fixture"]});
    write_json(&options.output.join("result.json"), &report)?;
    println!("PASS updater commands against pinned original baseline: merged output, nonzero status, preserved maintenance environment, and SIGINT group cleanup with live/exited leaders");
    Ok(report)
}
pub fn cli() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if matches!(args.as_slice(),[arg]if arg=="--help"||arg=="-h") {
        println!("Usage: check-updated-process --binary PATH --launcher PATH --fixture PATH --baseline PATH --output PATH");
        return Ok(());
    }
    run(&args)?;
    Ok(())
}

#[cfg(test)]
mod ownership_tests;
