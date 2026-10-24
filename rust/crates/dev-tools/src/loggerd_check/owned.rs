//! Dedicated-CLI ownership: unreaped leaders pin PGIDs; adopted children use
//! pidfds. IPC cleanup starts only after both classes have demonstrably exited.
use super::{receiver::Receiver, require, save, Result};
use rustix::process::{
    kill_process_group, pidfd_open, pidfd_send_signal, waitid, Pid, PidfdFlags, Signal, WaitId,
    WaitIdOptions,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::{self, File},
    os::{
        fd::OwnedFd,
        unix::{
            fs::{DirBuilderExt, MetadataExt},
            process::{CommandExt, ExitStatusExt},
        },
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
fn record(events: &mut Vec<Value>, mut value: Value) {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    value["monotonic_ns"] = json!(time.tv_sec as u64 * 1_000_000_000 + time.tv_nsec as u64);
    events.push(value);
}
fn children() -> Result<Vec<Pid>> {
    let owner = rustix::process::getpid().as_raw_pid();
    fs::read_to_string(format!("/proc/self/task/{owner}/children"))?
        .split_whitespace()
        .map(|s| Pid::from_raw(s.parse()?).ok_or_else(|| "invalid owned child PID".into()))
        .collect()
}
fn exited(pid: Pid) -> Result<bool> {
    match waitid(
        WaitId::Pid(pid),
        WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
    ) {
        Ok(status) => Ok(status.is_some()),
        Err(rustix::io::Errno::INTR) => Ok(false),
        Err(e) => Err(e.into()),
    }
}
fn signal(fd: &OwnedFd, signal: Signal) -> Result<()> {
    match pidfd_send_signal(fd, signal) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
fn reap(events: &mut Vec<Value>) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(8);
    let owner = rustix::process::getpid().as_raw_pid();
    let mut held: HashMap<Pid, (OwnedFd, Instant)> = HashMap::new();
    loop {
        let children = children()?;
        if children.is_empty() {
            record(events, json!({"phase":"adopted-children-empty"}));
            return Ok(());
        }
        for pid in children {
            let parent = fs::read_to_string(format!("/proc/{}/status", pid.as_raw_pid()))?
                .lines()
                .find_map(|l| l.strip_prefix("PPid:\t"))
                .ok_or("owned parent missing")?
                .trim()
                .parse::<i32>()?;
            require(parent == owner, "adopted process changed parent")?;
            if exited(pid)? {
                waitid(
                    WaitId::Pid(pid),
                    WaitIdOptions::EXITED | WaitIdOptions::NOHANG,
                )?;
                held.remove(&pid);
                record(
                    events,
                    json!({"phase":"adopted-reaped","pid":pid.as_raw_pid()}),
                );
                continue;
            }
            match held.entry(pid) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    let fd = pidfd_open(pid, PidfdFlags::empty())?;
                    signal(&fd, Signal::TERM)?;
                    record(
                        events,
                        json!({"phase":"adopted-signal","pid":pid.as_raw_pid(),"signal":15}),
                    );
                    entry.insert((fd, Instant::now()));
                }
                std::collections::hash_map::Entry::Occupied(entry) => {
                    let (fd, started) = entry.get();
                    if started.elapsed() >= Duration::from_secs(2) {
                        signal(fd, Signal::KILL)?;
                        record(
                            events,
                            json!({"phase":"adopted-signal","pid":pid.as_raw_pid(),"signal":9}),
                        );
                    }
                }
            }
        }
        require(
            Instant::now() < deadline,
            "logger QA adopted-child cleanup exceeded bound",
        )?;
        std::thread::sleep(Duration::from_millis(5));
    }
}
struct Namespace {
    path: PathBuf,
    held: File,
}
impl Namespace {
    fn new(prefix: &str) -> Result<Self> {
        let path = PathBuf::from(format!("/dev/shm/msgq_{prefix}"));
        fs::DirBuilder::new().mode(0o700).create(&path)?;
        let held = File::from(rustix::fs::open(
            &path,
            rustix::fs::OFlags::PATH
                | rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        Ok(Self { path, held })
    }
    fn receipt(&self) -> Result<Value> {
        let held = self.held.metadata()?;
        Ok(json!({"path":self.path,"dev":held.dev(),"ino":held.ino()}))
    }
    fn cleanup(self) -> Result<Value> {
        let current = fs::symlink_metadata(&self.path)?;
        let held = self.held.metadata()?;
        require(
            (current.dev(), current.ino()) == (held.dev(), held.ino()),
            "private msgq root replaced; preserved",
        )?;
        // Pre-created private namespace; no shared diagnostic leaf lives here.
        fs::remove_dir_all(&self.path)?;
        Ok(
            json!({"kind":"namespace","path":self.path,"dev":held.dev(),"ino":held.ino(),"status":"removed-owned"}),
        )
    }
}
struct Worker {
    child: Child,
    done: bool,
    code: Option<i32>,
    deadline: Instant,
}
impl Worker {
    fn end_group(&self, events: &mut Vec<Value>) -> Result<()> {
        let pid = Pid::from_child(&self.child);
        // ECHILD is an error: never signal a PGID after losing leader ownership.
        exited(pid)?;
        match kill_process_group(pid, Signal::KILL) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => {}
            Err(e) => return Err(e.into()),
        }
        // Also cover a leader that deliberately changed process group.
        let identity = pidfd_open(pid, PidfdFlags::empty())?;
        signal(&identity, Signal::KILL)?;
        record(
            events,
            json!({"phase":"worker-group-killed","pid":pid.as_raw_pid()}),
        );
        Ok(())
    }
    fn wait(&mut self, events: &mut Vec<Value>) -> Result<()> {
        if self.done {
            return Ok(());
        }
        let pid = Pid::from_child(&self.child);
        let until = Instant::now() + Duration::from_secs(8);
        while !exited(pid)? {
            require(
                Instant::now() < until,
                "logger direct-child cleanup exceeded bound",
            )?;
            std::thread::sleep(Duration::from_millis(5));
        }
        let status = self.child.wait()?;
        self.code = Some(
            status
                .code()
                .unwrap_or_else(|| -status.signal().unwrap_or(1)),
        );
        self.done = true;
        record(
            events,
            json!({"phase":"worker-reaped","pid":pid.as_raw_pid(),"exit":self.code}),
        );
        Ok(())
    }
}
pub(super) struct Owner {
    workers: Vec<Worker>,
    namespaces: Vec<Namespace>,
    receivers: Vec<Receiver>,
    endpoints: Vec<(PathBuf, crate::qa_endpoint::EndpointOwner)>,
    pub cancelled: Arc<AtomicBool>,
    signal_number: Arc<AtomicUsize>,
    handlers: Vec<signal_hook::SigId>,
    executable: PathBuf,
    control: PathBuf,
    sequence: u64,
    events: Vec<Value>,
    finished: bool,
    resource_errors: Vec<String>,
}
impl Owner {
    pub fn new(control: &Path) -> Result<Self> {
        Self::new_for(control, "check-loggerd-native")
    }
    pub(super) fn new_for(control: &Path, name: &str) -> Result<Self> {
        require(
            [
                "check-loggerd-native",
                "check-loggerd-peer",
                "check-loggerd-io",
                "check-loggerd-diagnostics",
                "check-loggerd-log-pipeline",
            ]
            .contains(&name),
            "unknown logger supervisor CLI",
        )?;
        let executable = std::env::current_exe()?;
        require(
            executable.file_name() == Some(std::ffi::OsStr::new(name)),
            "logger supervisor requires its dedicated CLI",
        )?;
        require(
            fs::read_dir("/proc/self/task")?.count() == 1 && children()?.is_empty(),
            "logger supervisor requires one thread and no pre-existing children",
        )?;
        rustix::process::set_child_subreaper(Some(rustix::process::getpid()))?;
        fs::create_dir(control)?;
        let mut owner = Self {
            workers: Vec::new(),
            namespaces: Vec::new(),
            receivers: Vec::new(),
            endpoints: Vec::new(),
            cancelled: Arc::new(AtomicBool::new(false)),
            signal_number: Arc::new(AtomicUsize::new(0)),
            handlers: Vec::new(),
            executable,
            control: control.into(),
            sequence: 0,
            events: Vec::new(),
            finished: false,
            resource_errors: Vec::new(),
        };
        for number in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            owner.handlers.push(signal_hook::flag::register_usize(
                number,
                owner.signal_number.clone(),
                number as usize,
            )?);
            owner.handlers.push(signal_hook::flag::register(
                number,
                owner.cancelled.clone(),
            )?);
        }
        Ok(owner)
    }
    pub(super) fn check_cancelled(&self) -> Result<()> {
        if self.cancelled.load(Ordering::Relaxed) {
            Err(super::Interrupted(self.signal_number.load(Ordering::Relaxed) as i32).into())
        } else {
            Ok(())
        }
    }
    fn receipt(&self, processes_confirmed: bool, error: Option<String>) -> Result<()> {
        save(
            &self.control.join("cleanup.json"),
            &json!({
            "processes_confirmed":processes_confirmed,"error":error,"events":self.events,
            "pending_namespaces":self.namespaces.len(),"pending_receivers":self.receivers.len(),"pending_endpoints":self.endpoints.len()}),
        )
    }
    fn resources(&mut self) -> Result<()> {
        let mut failure = None;
        // Attempt every independent cleanup even after a drain/receipt error.
        while let Some(receiver) = self.receivers.pop() {
            if let Err(e) = receiver.drain() {
                failure.get_or_insert(e);
            }
            match receiver.cleanup() {
                Ok(value) => record(
                    &mut self.events,
                    json!({"phase":"ipc-cleanup","resource":value}),
                ),
                Err(e) => {
                    failure.get_or_insert(e);
                }
            }
        }
        while let Some((path, owner)) = self.endpoints.pop() {
            if let Err(e) = owner.verify() {
                failure.get_or_insert(e);
            }
            drop(owner); // authenticated unlink only, after every process reaped
            let absent = fs::symlink_metadata(&path)
                .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound);
            record(
                &mut self.events,
                json!({"phase":"collector-endpoint-cleanup","path":path,"absent":absent}),
            );
            if !absent {
                failure.get_or_insert_with(|| "collector endpoint remains; preserved".into());
            }
        }
        while let Some(namespace) = self.namespaces.pop() {
            match namespace.cleanup() {
                Ok(value) => record(
                    &mut self.events,
                    json!({"phase":"ipc-cleanup","resource":value}),
                ),
                Err(e) => {
                    failure.get_or_insert(e);
                }
            }
        }
        if let Some(e) = failure {
            record(
                &mut self.events,
                json!({"phase":"resource-cleanup-error","error":e.to_string()}),
            );
            self.resource_errors.push(e.to_string());
        }
        let error = (!self.resource_errors.is_empty()).then(|| self.resource_errors.join("; "));
        self.receipt(true, error.clone())?;
        if let Some(error) = error {
            return Err(error.into());
        }
        Ok(())
    }
    pub fn run(&mut self, requests: &[super::Request], timeout: Duration) -> Result<Vec<Value>> {
        require(
            self.workers.is_empty()
                && self.namespaces.is_empty()
                && self.receivers.is_empty()
                && self.endpoints.is_empty(),
            "previous logger cohort still owned",
        )?;
        for request in requests {
            self.check_cancelled()?;
            self.sequence += 1;
            let prefix = format!(
                "rust-probe-logger-{}-{}",
                rustix::process::getpid().as_raw_pid(),
                self.sequence
            );
            self.namespaces.push(Namespace::new(&prefix)?);
            let endpoint_channel = if request.kind == "pipeline" {
                let path = PathBuf::from(format!("/tmp/logmessage{prefix}"));
                let endpoint = crate::qa_endpoint::EndpointOwner::new(&path, &prefix)?;
                let channel = endpoint.channel().to_owned();
                self.endpoints.push((path, endpoint));
                Some(channel)
            } else {
                None
            };
            let diagnostic =
                if !["fixture", "input-completion", "pipeline"].contains(&request.kind.as_str()) {
                    self.receivers
                        .push(Receiver::bind(&prefix, &request.output)?);
                    Some(
                        self.receivers
                            .last()
                            .ok_or("owned receiver missing")?
                            .receipt()?,
                    )
                } else {
                    None
                };
            let namespace = self
                .namespaces
                .last()
                .ok_or("owned namespace missing")?
                .receipt()?;
            save(
                &self
                    .control
                    .join(format!("{}.resources.json", self.sequence)),
                &json!({"namespace":namespace,"diagnostic":diagnostic}),
            )?;
            let bootstrap = self.control.join(format!("{}.json", self.sequence));
            save(&bootstrap, &serde_json::to_value(request)?)?;
            let stdout = File::create(self.control.join(format!("{}.stdout", self.sequence)))?;
            let stderr = File::create(self.control.join(format!("{}.stderr", self.sequence)))?;
            let mut command = Command::new(&self.executable);
            if let Some(channel) = endpoint_channel {
                command.env("LOGGERD_PIPELINE_ENDPOINT_OWNER", channel);
            }
            let child = command
                .arg(super::WORKER)
                .arg(&bootstrap)
                .env("OPENPILOT_PREFIX", &prefix)
                .stdin(Stdio::null())
                .stdout(stdout)
                .stderr(stderr)
                .process_group(0)
                .spawn()?;
            // Arm ownership before any further fallible work after spawn.
            self.workers.push(Worker {
                child,
                done: false,
                code: None,
                deadline: Instant::now() + timeout,
            });
            let worker = self.workers.last().ok_or("worker ownership missing")?;
            record(
                &mut self.events,
                json!({"phase":"worker-spawned","pid":worker.child.id(),"sequence":self.sequence}),
            );
        }
        let fallback_cohort = !requests.is_empty() && requests.iter().all(|r| r.kind == "fallback");
        let mut released = false;
        loop {
            self.check_cancelled()?;
            if fallback_cohort
                && !released
                && requests
                    .iter()
                    .all(|r| r.output.join("fallback-rotated.json").is_file())
            {
                for request in requests {
                    fs::write(request.output.join("fallback-stop"), b"cohort complete")?;
                }
                released = true;
            }
            let mut all = true;
            for worker in &mut self.workers {
                if worker.done {
                    continue;
                }
                all = false;
                if exited(Pid::from_child(&worker.child))? {
                    worker.end_group(&mut self.events)?;
                    worker.wait(&mut self.events)?;
                    require(
                        worker.code == Some(0),
                        "logger QA worker failed; retained control stderr/result",
                    )?;
                } else {
                    require(
                        Instant::now() < worker.deadline,
                        "logger QA worker exceeded bound",
                    )?;
                }
            }
            if all {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        self.workers.clear();
        reap(&mut self.events)?;
        self.resources()?;
        requests
            .iter()
            .map(|r| {
                Ok(serde_json::from_slice(&fs::read(
                    r.output.join("worker-result.json"),
                )?)?)
            })
            .collect()
    }
    pub fn finish(&mut self) -> Result<()> {
        require(
            self.workers.is_empty(),
            "logger workers remain at completion",
        )?;
        reap(&mut self.events)?;
        self.resources()?;
        self.check_cancelled()?;
        self.finished = true;
        Ok(())
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        if !self.finished {
            let mut failure = None;
            for worker in &mut self.workers {
                if worker.done {
                    continue;
                }
                if let Err(e) = worker
                    .end_group(&mut self.events)
                    .and_then(|()| worker.wait(&mut self.events))
                {
                    failure.get_or_insert(e);
                }
            }
            if failure.is_none() {
                if let Err(e) = reap(&mut self.events) {
                    failure = Some(e);
                }
            }
            if let Some(error) = failure {
                eprintln!("logger QA cleanup not confirmed; IPC resources preserved: {error}");
                let _ = self.receipt(false, Some(error.to_string()));
                for receiver in self.receivers.drain(..) {
                    receiver.preserve();
                }
                self.namespaces.clear(); // No unlinking Drop.
                for (_, endpoint) in self.endpoints.drain(..) {
                    std::mem::forget(endpoint);
                } // preserve when process cleanup is uncertain
            } else if let Err(e) = self.resources() {
                eprintln!("logger QA resource cleanup failed: {e}");
            }
        }
        for handler in self.handlers.drain(..) {
            signal_hook::low_level::unregister(handler);
        }
    }
}
