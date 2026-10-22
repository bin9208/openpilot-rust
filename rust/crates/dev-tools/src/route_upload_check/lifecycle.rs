//! Family-local subreaper supervisor. Only its sole worker can create children.
//! Resource notices are acknowledged while the worker retains each original FD.
use super::{require, runtime::HeldPath, Options};
use crate::{
    qa_process::{Captured, CommandSpec},
    seal::Result,
};
use rustix::{
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
    process::{
        kill_process_group, pidfd_open, pidfd_send_signal, waitid, Pid, PidfdFlags, Signal, WaitId,
        WaitIdOptions,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs::{self, File},
    io::{BufRead, Read, Write},
    os::{
        fd::OwnedFd,
        unix::{
            ffi::{OsStrExt, OsStringExt},
            fs::MetadataExt,
            process::{CommandExt, ExitStatusExt},
        },
    },
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
pub(super) const WORKER: &str = "--internal-route-upload-worker";
#[derive(Debug)]
pub struct Interrupted(pub i32);
impl std::fmt::Display for Interrupted {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(out, "route-upload interrupted by signal {}", self.0)
    }
}
impl std::error::Error for Interrupted {}
pub(super) struct Cancel {
    pub(super) flag: Arc<AtomicBool>,
    signal: Arc<AtomicUsize>,
    handlers: Vec<signal_hook::SigId>,
}
impl Cancel {
    fn new() -> Result<Self> {
        let mut cancel = Self {
            flag: Arc::new(AtomicBool::new(false)),
            signal: Arc::new(AtomicUsize::new(0)),
            handlers: Vec::new(),
        };
        for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            cancel.handlers.push(signal_hook::flag::register_usize(
                signal,
                cancel.signal.clone(),
                signal as usize,
            )?);
            cancel
                .handlers
                .push(signal_hook::flag::register(signal, cancel.flag.clone())?);
        }
        Ok(cancel)
    }
    fn number(&self) -> i32 {
        self.signal.load(Ordering::Relaxed) as i32
    }
    fn check(&self) -> Result<()> {
        if self.flag.load(Ordering::Relaxed) {
            Err(Box::new(Interrupted(self.number())))
        } else {
            Ok(())
        }
    }
}
impl Drop for Cancel {
    fn drop(&mut self) {
        for handler in self.handlers.drain(..) {
            signal_hook::low_level::unregister(handler);
        }
    }
}
#[derive(Serialize, Deserialize)]
struct Bootstrap {
    manager: bool,
    root: Vec<u8>,
    binaries: Vec<u8>,
    probe: Option<Vec<u8>>,
    output: Vec<u8>,
}
impl Bootstrap {
    fn new(manager: bool, options: &Options) -> Self {
        Self {
            manager,
            root: options.root.as_os_str().as_bytes().into(),
            binaries: options.binaries.as_os_str().as_bytes().into(),
            probe: options
                .probe
                .as_ref()
                .map(|p| p.as_os_str().as_bytes().into()),
            output: options.output.as_os_str().as_bytes().into(),
        }
    }
    fn options(self) -> Options {
        let path = |bytes| PathBuf::from(std::ffi::OsString::from_vec(bytes));
        Options {
            root: path(self.root),
            binaries: path(self.binaries),
            probe: self.probe.map(path),
            output: path(self.output),
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum Message {
    Ready,
    Own {
        path: Vec<u8>,
        dev: u64,
        ino: u64,
        directory: bool,
        receipt: bool,
    },
    Result {
        text: String,
    },
}
pub(super) struct Context {
    pub(super) cancel: Cancel,
    observed: BTreeMap<PathBuf, (u64, u64)>,
    probe_pid: Option<Pid>,
    probe_started: bool,
    probe_namespace: Option<HeldPath>,
    ipc_root: Option<HeldPath>,
}
impl Context {
    fn send(&self, message: &Message) -> Result<()> {
        let mut bytes = serde_json::to_vec(message)?;
        bytes.push(b'\n');
        std::io::stdout().write_all(&bytes)?;
        std::io::stdout().flush()?;
        let mut ack = [0];
        std::io::stdin().read_exact(&mut ack)?;
        require(
            ack == [1],
            "native supervisor did not acknowledge ownership",
        )
    }
    pub(super) fn check(&self) -> Result<()> {
        self.cancel.check()
    }
    pub(super) fn sleep(&self, duration: Duration) -> Result<()> {
        let deadline = Instant::now() + duration;
        loop {
            self.check()?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(());
            }
            thread::sleep(remaining.min(Duration::from_millis(10)));
        }
    }
    pub(super) fn own(&mut self, path: &HeldPath) -> Result<()> {
        if let Some(root) = self
            .ipc_root
            .as_ref()
            .filter(|root| path.path != root.path && path.path.starts_with(&root.path))
        {
            require(
                root.matches_path()?,
                "QA IPC root replaced before socket ownership",
            )?;
        }
        let meta = path.held.metadata()?;
        self.send(&Message::Own {
            path: path.path.as_os_str().as_bytes().into(),
            dev: meta.dev(),
            ino: meta.ino(),
            directory: path.directory,
            receipt: path.receipt,
        })?;
        self.observed
            .insert(path.path.clone(), (meta.dev(), meta.ino()));
        Ok(())
    }
    pub(super) fn watch_manager(&mut self) -> Result<()> {
        self.prepare_ipc()?;
        let namespace = tempfile::Builder::new()
            .prefix("msgq_rust-probe-installed-")
            .tempdir_in("/dev/shm")?;
        let held = HeldPath::new(namespace.path(), true)?;
        let _ = namespace.keep();
        self.own(&held)?;
        self.probe_namespace = Some(held);
        Ok(())
    }
    pub(super) fn probe_namespace(&self) -> Result<&Path> {
        Ok(&self
            .probe_namespace
            .as_ref()
            .ok_or("probe namespace missing")?
            .path)
    }
    pub(super) fn release_probe_namespace(&mut self) -> Result<()> {
        let namespace = self
            .probe_namespace
            .take()
            .ok_or("probe namespace missing")?;
        require(
            namespace.matches_path()?,
            "probe namespace replaced before uploader setup",
        )?;
        let path = namespace.path.clone();
        drop(namespace);
        require(!path.try_exists()?, "probe namespace cleanup failed")
    }
    pub(super) fn prepare_ipc(&mut self) -> Result<PathBuf> {
        require(self.ipc_root.is_none(), "duplicate QA IPC namespace")?;
        let root = tempfile::Builder::new()
            .prefix("route-upload-ipc-")
            .tempdir_in("/tmp")?;
        let mut held = HeldPath::new(root.path(), true)?;
        let path = root.keep();
        self.own(&held)?;
        // Only the outer owner removes this private contents-owned namespace,
        // after every worker/adopted child is reaped, including bind-then-exit.
        held.preserve = true;
        self.ipc_root = Some(held);
        Ok(path)
    }
    pub(super) fn ipc_root(&self) -> Result<&Path> {
        Ok(&self.ipc_root.as_ref().ok_or("QA IPC root missing")?.path)
    }
    fn observe_manager(&mut self) -> Result<()> {
        let Some(root) = &self.ipc_root else {
            return Ok(());
        };
        let paths = [root.path.join("logmessage"), root.path.join("stats")];
        for path in paths {
            let meta = match fs::symlink_metadata(&path) {
                Ok(meta) => meta,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            if self.observed.get(&path) == Some(&(meta.dev(), meta.ino())) {
                continue;
            }
            if let Some(pid) = self.probe_pid {
                let mut held = HeldPath::new(&path, false)?;
                held.preserve = true;
                if descendant_socket(pid, &path)? && held.matches_path()? {
                    self.own(&held)?;
                }
            }
        }
        Ok(())
    }
    pub(super) fn publish(&mut self, path: &Path, text: &str) -> Result<()> {
        self.check()?;
        let file = File::options().write(true).create_new(true).open(path)?;
        let mut held = HeldPath::receipt(path, file)?;
        self.own(&held)?;
        held.held.write_all(text.as_bytes())?;
        held.held.write_all(b"\n")?;
        held.held.flush()?;
        self.check()?;
        require(
            held.matches_path()?,
            "success receipt replaced during publication",
        )?;
        held.preserve = true;
        Ok(())
    }
}
pub(super) fn worker(manager: bool) -> Result<()> {
    // Install handlers before creating storage or native children. The outer
    // supervisor never forwards cancellation until Ready is acknowledged.
    let cancel = Cancel::new()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    require(line.len() <= 256 * 1024, "native bootstrap exceeds bound")?;
    let bootstrap: Bootstrap = serde_json::from_str(&line)?;
    require(bootstrap.manager == manager, "wrong native worker role")?;
    let options = bootstrap.options();
    let mut context = Context {
        cancel,
        observed: BTreeMap::new(),
        probe_pid: None,
        probe_started: false,
        probe_namespace: None,
        ipc_root: None,
    };
    context.send(&Message::Ready)?;
    context.check()?;
    let text = if manager {
        super::manager(&options, &mut context)?
    } else {
        super::runtime(&options, &mut context)?
    };
    context.check()?;
    context.send(&Message::Result { text })?;
    context.check()
}
fn exited(pid: Pid) -> Result<bool> {
    match waitid(
        WaitId::Pid(pid),
        WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
    ) {
        Ok(status) => Ok(status.is_some()),
        Err(rustix::io::Errno::INTR) => Ok(false),
        Err(error) => Err(error.into()),
    }
}
struct Group {
    child: Child,
    reaped: bool,
}
impl Drop for Group {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = kill_process_group(Pid::from_child(&self.child), Signal::KILL);
            let _ = self.child.wait();
        }
    }
}
pub(super) fn capture(spec: &CommandSpec, path: &Path, context: &mut Context) -> Result<Captured> {
    let argv = spec.effective_argv();
    let (program, args) = argv.split_first().ok_or("empty route command")?;
    let output = File::create(path)?;
    let mut command = Command::new(program);
    command
        .args(args)
        .envs(spec.effective_env())
        .stdin(Stdio::null())
        .stdout(output.try_clone()?)
        .stderr(output)
        .process_group(0);
    for name in &spec.env_remove {
        command.env_remove(name);
    }
    if let Some(cwd) = &spec.cwd {
        command.current_dir(cwd);
    }
    let mut group = Group {
        child: command.spawn()?,
        reaped: false,
    };
    if context.probe_namespace.is_some() && !context.probe_started {
        context.probe_pid = Some(Pid::from_child(&group.child));
        context.probe_started = true;
    }
    let deadline = Instant::now() + spec.timeout;
    loop {
        context.observe_manager()?;
        context.check()?;
        if exited(Pid::from_child(&group.child))? {
            // Observe final IPC paths before releasing the probe's leader.
            context.observe_manager()?;
            match kill_process_group(Pid::from_child(&group.child), Signal::KILL) {
                Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                Err(error) => return Err(error.into()),
            }
            let status = group.child.wait()?;
            group.reaped = true;
            context.probe_pid = None;
            return Ok(Captured {
                code: status
                    .code()
                    .unwrap_or_else(|| -status.signal().unwrap_or(1)),
                stdout: Vec::new(),
                stderr: Vec::new(),
            });
        }
        require(Instant::now() < deadline, "route command timed out")?;
        thread::sleep(Duration::from_millis(10));
    }
}
// A shared pathname is never enough to claim a socket. Match the live kernel
// socket to an FD in this unreaped probe's owned tree, retaining pidfds while
// checking ancestry. Ambiguous/unlinked-and-rebound names remain unclaimed.
fn descendant_socket(root: Pid, path: &Path) -> Result<bool> {
    let network = fs::read_to_string("/proc/net/unix")?;
    let sockets = network
        .lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            (fields.len() == 8
                // Accepted stream sockets inherit the listener's pathname.
                // Only SO_ACCEPTCON identifies the live listening endpoint.
                && fields[3] == "00010000"
                && fields[4] == "0001"
                && fields[7].as_bytes() == path.as_os_str().as_bytes())
            .then(|| fields[6].to_owned())
        })
        .collect::<Vec<_>>();
    if sockets.len() != 1 {
        return Ok(false);
    }
    let expected = format!("socket:[{}]", sockets[0]);
    let mut queue = vec![(root, None)];
    let mut identities = Vec::new();
    let mut seen = BTreeSet::new();
    while let Some((pid, expected_parent)) = queue.pop() {
        if !seen.insert(pid.as_raw_pid()) {
            continue;
        }
        let Ok(identity) = pidfd_open(pid, PidfdFlags::empty()) else {
            continue;
        };
        if expected_parent.is_some_and(|owner| parent(pid).ok() != Some(owner)) {
            continue;
        }
        identities.push(identity);
        let directory = format!("/proc/{}", pid.as_raw_pid());
        if let Ok(entries) = fs::read_dir(format!("{directory}/fd")) {
            for entry in entries.flatten() {
                if fs::read_link(entry.path())
                    .is_ok_and(|target| target.as_os_str() == std::ffi::OsStr::new(&expected))
                    && expected_parent.is_none_or(|owner| parent(pid).ok() == Some(owner))
                {
                    return Ok(true);
                }
            }
        }
        if let Ok(tasks) = fs::read_dir(format!("{directory}/task")) {
            for task in tasks.flatten() {
                if let Ok(children) = fs::read_to_string(task.path().join("children")) {
                    for child in children.split_whitespace() {
                        if let Some(child) = child.parse().ok().and_then(Pid::from_raw) {
                            queue.push((child, Some(pid.as_raw_pid())));
                        }
                    }
                }
            }
        }
    }
    Ok(false)
}
fn direct_children() -> Result<Vec<Pid>> {
    let owner = rustix::process::getpid().as_raw_pid();
    fs::read_to_string(format!("/proc/self/task/{owner}/children"))?
        .split_whitespace()
        .map(|text| Pid::from_raw(text.parse()?).ok_or_else(|| "invalid adopted child PID".into()))
        .collect()
}
fn parent(pid: Pid) -> Result<i32> {
    fs::read_to_string(format!("/proc/{}/status", pid.as_raw_pid()))?
        .lines()
        .find_map(|line| line.strip_prefix("PPid:\t"))
        .ok_or("child parent identity missing")?
        .trim()
        .parse()
        .map_err(Into::into)
}
fn signal(identity: &OwnedFd, value: Signal) -> Result<()> {
    match pidfd_send_signal(identity, value) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(error) => Err(error.into()),
    }
}
fn reap_adopted(worker: Pid) -> Result<bool> {
    let owner = rustix::process::getpid().as_raw_pid();
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut owned: HashMap<Pid, (OwnedFd, Instant)> = HashMap::new();
    let mut saw_child = false;
    loop {
        let children = direct_children()?
            .into_iter()
            .filter(|pid| *pid != worker)
            .collect::<Vec<_>>();
        if children.is_empty() {
            return Ok(saw_child);
        }
        saw_child = true;
        for child in children {
            require(parent(child)? == owner, "adopted child changed parent")?;
            if exited(child)? {
                // Reap only after any final pidfd signal; never retain a PID to
                // signal after wait has released its kernel identity.
                owned.remove(&child);
                waitid(
                    WaitId::Pid(child),
                    WaitIdOptions::EXITED | WaitIdOptions::NOHANG,
                )?;
                continue;
            }
            match owned.entry(child) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    let identity = pidfd_open(child, PidfdFlags::empty())?;
                    require(parent(child)? == owner, "adopted child identity changed")?;
                    signal(&identity, Signal::TERM)?;
                    entry.insert((identity, Instant::now()));
                }
                std::collections::hash_map::Entry::Occupied(entry) => {
                    let (identity, started) = entry.get();
                    if started.elapsed() >= Duration::from_secs(2) {
                        signal(identity, Signal::KILL)?;
                    }
                }
            }
        }
        require(
            Instant::now() < deadline,
            "owned adopted-child cleanup exceeded bound",
        )?;
        thread::sleep(Duration::from_millis(10));
    }
}
struct Supervisor {
    child: Child,
    identity: OwnedFd,
    reaped: bool,
    input: ChildStdin,
    stream: File,
    pending: Vec<u8>,
    resources: Vec<HeldPath>,
    journal: Option<File>,
}
struct Starting(Option<Child>);
impl Drop for Starting {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = kill_process_group(Pid::from_child(child), Signal::KILL);
            let _ = child.wait();
        }
    }
}
impl Drop for Supervisor {
    fn drop(&mut self) {
        let mut cleanup_confirmed = self.reaped;
        if !self.reaped {
            let _ = signal(&self.identity, Signal::KILL);
            // Keep the worker's PID reserved until all adopted children have
            // been confirmed reaped. A failed wait is never cleanup authority.
            let deadline = Instant::now() + Duration::from_secs(2);
            let worker_exited = loop {
                match exited(Pid::from_child(&self.child)) {
                    Ok(true) => break true,
                    Ok(false) if Instant::now() < deadline => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    _ => break false,
                }
            };
            if worker_exited {
                match reap_adopted(Pid::from_child(&self.child)) {
                    Ok(_) => cleanup_confirmed = self.child.wait().is_ok(),
                    Err(error) => eprintln!(
                        "route-upload adopted cleanup: {}",
                        error.to_string().chars().take(512).collect::<String>()
                    ),
                }
            }
        }
        if !cleanup_confirmed {
            for resource in &mut self.resources {
                resource.preserve = true;
            }
            eprintln!("route-upload: IPC preserved because complete owned-child reaping could not be confirmed");
            return;
        }
        let mut latest = BTreeMap::new();
        for resource in &self.resources {
            if !resource.directory && !resource.receipt {
                latest.insert(resource.path.clone(), resource);
            }
        }
        let foreign = latest
            .into_iter()
            .filter_map(|(path, held)| match held.matches_path() {
                Ok(false) if fs::symlink_metadata(&path).is_ok() => Some(path),
                Err(_) => Some(path),
                _ => None,
            })
            .collect::<Vec<_>>();
        for resource in &mut self.resources {
            if resource.directory && foreign.iter().any(|path| path.starts_with(&resource.path)) {
                resource.preserve = true;
            }
        }
        while let Some(resource) = self.resources.pop() {
            drop(resource);
        }
    }
}
impl Supervisor {
    fn messages(&mut self) -> Result<(Vec<Message>, bool)> {
        let mut bytes = [0; 16384];
        let eof = match self.stream.read(&mut bytes) {
            Ok(0) => true,
            Ok(count) => {
                self.pending.extend_from_slice(&bytes[..count]);
                false
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                false
            }
            Err(error) => return Err(error.into()),
        };
        require(
            self.pending.len() <= 1024 * 1024,
            "native ownership message exceeds bound",
        )?;
        let mut messages = Vec::new();
        while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line = self.pending.drain(..=end).collect::<Vec<_>>();
            messages.push(serde_json::from_slice(&line)?);
        }
        if eof {
            require(
                self.pending.is_empty(),
                "truncated native ownership message",
            )?;
        }
        Ok((messages, eof))
    }
    fn accept(
        &mut self,
        message: Message,
        ready: &mut bool,
        result: &mut Option<String>,
        output: &Path,
    ) -> Result<()> {
        match message {
            Message::Ready => {
                require(!*ready, "duplicate native worker readiness")?;
                *ready = true;
            }
            Message::Result { text } => {
                require(*ready, "native result before readiness")?;
                require(result.is_none(), "duplicate native result")?;
                *result = Some(text);
            }
            Message::Own {
                path,
                dev,
                ino,
                directory,
                receipt,
            } => {
                require(*ready, "ownership before native readiness")?;
                let path = PathBuf::from(std::ffi::OsString::from_vec(path));
                let mut owned = if receipt {
                    require(
                        path == output.join("result.json"),
                        "unowned success receipt path",
                    )?;
                    let fd = rustix::fs::open(
                        &path,
                        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                        rustix::fs::Mode::empty(),
                    )?;
                    HeldPath::receipt(&path, File::from(fd))?
                } else {
                    HeldPath::new(&path, directory)?
                };
                // A failed identity acknowledgement must preserve the foreign
                // path just opened, rather than allowing its guard to unlink it.
                owned.preserve = true;
                let meta = owned.held.metadata()?;
                require(
                    (meta.dev(), meta.ino()) == (dev, ino),
                    "ownership path replaced before FD acknowledgement",
                )?;
                owned.preserve = false;
                self.resources.push(owned);
                if self.journal.is_none() {
                    self.journal = Some(
                        File::options()
                            .write(true)
                            .create_new(true)
                            .open(output.join("ownership.jsonl"))?,
                    );
                }
                if let Some(journal) = &mut self.journal {
                    let mut record = serde_json::to_vec(&Message::Own {
                        path: self
                            .resources
                            .last()
                            .ok_or("ownership missing")?
                            .path
                            .as_os_str()
                            .as_bytes()
                            .into(),
                        dev,
                        ino,
                        directory,
                        receipt,
                    })?;
                    record.push(b'\n');
                    journal.write_all(&record)?;
                    journal.flush()?;
                }
            }
        }
        self.input.write_all(&[1])?;
        Ok(())
    }
}
pub(super) fn supervise(manager: bool, options: &Options) -> Result<String> {
    // This dedicated executable process creates exactly one worker. No global
    // library caller or unrelated process tree is made a cleanup target.
    let expected = if manager {
        "check-manager-log-upload"
    } else {
        "check-runtime-log-upload"
    };
    require(
        std::env::current_exe()?.file_name() == Some(std::ffi::OsStr::new(expected)),
        "route supervisor requires its dedicated CLI executable",
    )?;
    require(
        fs::read_dir("/proc/self/task")?.count() == 1 && direct_children()?.is_empty(),
        "route supervisor requires one thread and no pre-existing children",
    )?;
    rustix::process::set_child_subreaper(Some(rustix::process::getpid()))?;
    let cancel = Cancel::new()?;
    let mut starting = Starting(Some(
        Command::new(std::env::current_exe()?)
            .arg(WORKER)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .process_group(0)
            .spawn()?,
    ));
    let child = starting.0.as_mut().ok_or("native worker missing")?;
    let identity = pidfd_open(Pid::from_child(child), PidfdFlags::empty())?;
    let input = child.stdin.take().ok_or("native worker stdin missing")?;
    let stream = File::from(OwnedFd::from(
        child.stdout.take().ok_or("native worker stdout missing")?,
    ));
    fcntl_setfl(&stream, fcntl_getfl(&stream)? | OFlags::NONBLOCK)?;
    let child = starting.0.take().ok_or("native worker ownership lost")?;
    let mut owner = Supervisor {
        child,
        identity,
        reaped: false,
        input,
        stream,
        pending: Vec::new(),
        resources: Vec::new(),
        journal: None,
    };
    let mut bootstrap = serde_json::to_vec(&Bootstrap::new(manager, options))?;
    bootstrap.push(b'\n');
    owner.input.write_all(&bootstrap)?;
    let mut ready = false;
    let mut result = None;
    let mut cancelled_at = None;
    let deadline = Instant::now() + Duration::from_secs(150);
    loop {
        let (messages, eof) = owner.messages()?;
        for message in messages {
            owner.accept(message, &mut ready, &mut result, &options.output)?;
        }
        if ready && cancel.flag.load(Ordering::Relaxed) && cancelled_at.is_none() {
            signal(
                &owner.identity,
                if cancel.number() == signal_hook::consts::SIGINT {
                    Signal::INT
                } else {
                    Signal::TERM
                },
            )?;
            cancelled_at = Some(Instant::now());
        }
        if cancelled_at.is_some_and(|at: Instant| at.elapsed() >= Duration::from_secs(10)) {
            signal(&owner.identity, Signal::KILL)?;
        }
        if exited(Pid::from_child(&owner.child))? && eof {
            break;
        }
        require(
            Instant::now() < deadline,
            "native ownership supervisor deadline",
        )?;
        thread::sleep(Duration::from_millis(5));
    }
    let adopted = reap_adopted(Pid::from_child(&owner.child))?;
    let status = owner.child.wait()?;
    owner.reaped = true;
    cancel.check()?;
    require(
        status.success(),
        "native route worker failed; see its diagnostics",
    )?;
    require(!adopted, "successful native worker left adopted children")?;
    let text = result.ok_or("native worker omitted complete result")?;
    let receipts = owner
        .resources
        .iter_mut()
        .filter(|resource| resource.receipt)
        .collect::<Vec<_>>();
    require(receipts.len() == 1, "complete held success receipt missing")?;
    for receipt in receipts {
        require(
            receipt.matches_path()?,
            "success receipt replaced before supervisor acceptance",
        )?;
        let mut bytes = Vec::new();
        Read::by_ref(&mut receipt.held)
            .take(16 * 1024 * 1024)
            .read_to_end(&mut bytes)?;
        require(
            bytes == format!("{text}\n").as_bytes(),
            "held success receipt content changed",
        )?;
        cancel.check()?;
        receipt.preserve = true;
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn connected_unix_streams_keep_the_owned_listener_authenticatable() {
        use std::os::unix::net::{UnixListener, UnixStream};
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("logmessage");
        let listener = UnixListener::bind(&path).unwrap();
        let client = UnixStream::connect(&path).unwrap();
        let (accepted, _) = listener.accept().unwrap();
        let network = fs::read_to_string("/proc/net/unix").unwrap();
        assert!(
            network
                .lines()
                .filter(|line| line.split_whitespace().last() == path.to_str())
                .count()
                >= 2,
            "regression requires accepted socket pathname duplication"
        );
        assert!(
            descendant_socket(rustix::process::getpid(), &path).unwrap(),
            "connected peers must not hide the owned listening socket"
        );
        fs::remove_file(&path).unwrap();
        let replacement = UnixListener::bind(&path).unwrap();
        assert!(
            !descendant_socket(rustix::process::getpid(), &path).unwrap(),
            "two live listeners sharing a pathname remain ambiguous"
        );
        drop((replacement, accepted, client, listener));
    }
}
