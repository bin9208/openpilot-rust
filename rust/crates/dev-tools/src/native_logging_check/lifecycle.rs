//! Dedicated-CLI cancellation and adopted-child cleanup; no generic runner change.
use super::{require, Result};
use crate::qa_process::{Captured, CommandSpec, NativeRunner};
use rustix::process::{
    kill_process_group, pidfd_open, pidfd_send_signal, waitid, Pid, PidfdFlags, Signal, WaitId,
    WaitIdOptions,
};
use std::{
    collections::HashMap,
    fs,
    os::fd::OwnedFd,
    process::Child,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, OnceLock,
    },
    time::{Duration, Instant},
};
static CANCELLED: OnceLock<Arc<AtomicBool>> = OnceLock::new();
static NUMBER: OnceLock<Arc<AtomicUsize>> = OnceLock::new();
static CLEANUP_FAILED: AtomicBool = AtomicBool::new(false);
fn flag() -> &'static Arc<AtomicBool> {
    CANCELLED.get_or_init(|| Arc::new(AtomicBool::new(false)))
}
fn number() -> &'static Arc<AtomicUsize> {
    NUMBER.get_or_init(|| Arc::new(AtomicUsize::new(0)))
}
#[derive(Debug)]
pub struct Interrupted(pub i32);
impl std::fmt::Display for Interrupted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "interrupted by signal {}", self.0)
    }
}
impl std::error::Error for Interrupted {}
pub(super) fn check() -> Result<()> {
    if flag().load(Ordering::Relaxed) {
        Err(Box::new(Interrupted(
            number().load(Ordering::Relaxed) as i32
        )))
    } else {
        Ok(())
    }
}
pub(super) fn cleanup_confirmed() -> bool {
    !CLEANUP_FAILED.load(Ordering::Relaxed)
}
fn children() -> Result<Vec<Pid>> {
    let owner = rustix::process::getpid().as_raw_pid();
    fs::read_to_string(format!("/proc/self/task/{owner}/children"))?
        .split_whitespace()
        .map(|s| {
            Pid::from_raw(s.parse()?).ok_or_else(|| "invalid logging adopted-child PID".into())
        })
        .collect()
}
fn send(fd: &OwnedFd, sig: Signal) -> Result<()> {
    match pidfd_send_signal(fd, sig) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
fn reap() -> Result<()> {
    let until = Instant::now() + Duration::from_secs(8);
    let owner = rustix::process::getpid().as_raw_pid();
    let mut identities: HashMap<Pid, (OwnedFd, Instant)> = HashMap::new();
    loop {
        let children = children()?;
        if children.is_empty() {
            return Ok(());
        }
        for pid in children {
            let parent = fs::read_to_string(format!("/proc/{}/status", pid.as_raw_pid()))?
                .lines()
                .find_map(|l| l.strip_prefix("PPid:\t"))
                .ok_or("logging child parent missing")?
                .trim()
                .parse::<i32>()?;
            require(parent == owner, "logging adopted child changed parent")?;
            if waitid(
                WaitId::Pid(pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            )?
            .is_some()
            {
                identities.remove(&pid);
                waitid(
                    WaitId::Pid(pid),
                    WaitIdOptions::EXITED | WaitIdOptions::NOHANG,
                )?;
                continue;
            }
            match identities.entry(pid) {
                std::collections::hash_map::Entry::Vacant(e) => {
                    let fd = pidfd_open(pid, PidfdFlags::empty())?;
                    send(&fd, Signal::TERM)?;
                    e.insert((fd, Instant::now()));
                }
                std::collections::hash_map::Entry::Occupied(e) => {
                    let (fd, start) = e.get();
                    if start.elapsed() >= Duration::from_secs(2) {
                        send(fd, Signal::KILL)?;
                    }
                }
            }
        }
        require(
            Instant::now() < until,
            "logging adopted-child cleanup exceeded bound",
        )?;
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn group(pid: Pid) -> Result<()> {
    match kill_process_group(pid, Signal::KILL) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
fn retain(error: &dyn std::fmt::Display) {
    CLEANUP_FAILED.store(true, Ordering::Relaxed);
    eprintln!(
        "logging cleanup not confirmed; IPC paths preserved: {}",
        error.to_string().chars().take(512).collect::<String>()
    );
}
fn exited(pid: Pid) -> Result<bool> {
    match waitid(
        WaitId::Pid(pid),
        WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
    ) {
        Ok(status) => Ok(status.is_some()),
        // Let the caller check its cancellation/deadline before retrying.
        Err(rustix::io::Errno::INTR) => Ok(false),
        Err(error) => Err(error.into()),
    }
}
pub(super) fn cleanup(child: &mut Child) -> Result<()> {
    // Every caller retains this direct leader unreaped. Its PID/PGID cannot have
    // been recycled; wait releases that identity only after the final signal.
    let result = (|| {
        let pid = Pid::from_child(child);
        // First prove it is still our unreaped child, even on an error path.
        exited(pid)?;
        group(pid)?;
        let until = Instant::now() + Duration::from_secs(8);
        while !exited(pid)? {
            require(
                Instant::now() < until,
                "logging direct-child cleanup exceeded bound",
            )?;
            std::thread::sleep(Duration::from_millis(5));
        }
        // WNOWAIT has proved exit, so this wait only releases the zombie.
        child.wait()?;
        reap()
    })();
    if let Err(e) = &result {
        retain(&**e);
    }
    result
}
pub(super) fn wait(child: &mut Child, timeout: Duration) -> Result<i32> {
    use std::os::unix::process::ExitStatusExt;
    let until = Instant::now() + timeout;
    let pid = Pid::from_child(child);
    loop {
        check()?;
        if exited(pid)? {
            group(pid)?;
            let status = child.wait()?;
            // Tell the caller the leader has reaped even if adoption cleanup fails.
            // The caller clears Child ownership before invoking the separate reap step.
            return Ok(status
                .code()
                .unwrap_or_else(|| -status.signal().unwrap_or(1)));
        }
        require(Instant::now() < until, "producer shutdown timeout")?;
        std::thread::sleep(Duration::from_millis(1));
    }
}
pub(super) fn reaped() -> Result<()> {
    let result = reap();
    if let Err(e) = &result {
        retain(&**e);
    }
    result
}
pub(super) fn run(spec: &CommandSpec) -> Result<Captured> {
    check()?;
    let result = NativeRunner.run_controlled(spec, Instant::now() + spec.timeout, flag());
    reaped()?;
    check()?;
    result
}
pub(super) struct Cancel {
    handlers: Vec<signal_hook::SigId>,
}
impl Cancel {
    pub fn new() -> Result<Self> {
        require(
            std::env::current_exe()?.file_name()
                == Some(std::ffi::OsStr::new("check-native-logging")),
            "logging supervisor requires its dedicated CLI",
        )?;
        require(
            fs::read_dir("/proc/self/task")?.count() == 1 && children()?.is_empty(),
            "logging supervisor requires one thread and no pre-existing children",
        )?;
        rustix::process::set_child_subreaper(Some(rustix::process::getpid()))?;
        let mut result = Self {
            handlers: Vec::new(),
        };
        for sig in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            result.handlers.push(signal_hook::flag::register_usize(
                sig,
                number().clone(),
                sig as usize,
            )?);
            result
                .handlers
                .push(signal_hook::flag::register(sig, flag().clone())?);
        }
        Ok(result)
    }
}
impl Drop for Cancel {
    fn drop(&mut self) {
        if let Err(e) = reaped() {
            retain(&*e);
        }
        for h in self.handlers.drain(..) {
            signal_hook::low_level::unregister(h);
        }
    }
}
/// No resource is removed on an unconfirmed process-cleanup error path.
pub(super) struct Directory(Option<tempfile::TempDir>);
impl Directory {
    pub fn new(directory: tempfile::TempDir) -> Self {
        Self(Some(directory))
    }
    pub fn path(&self) -> &std::path::Path {
        self.0.as_ref().expect("owned directory retained").path()
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        if !cleanup_confirmed() {
            if let Some(directory) = self.0.take() {
                let _ = directory.keep();
            }
        }
    }
}
pub(super) struct Leaf(Option<super::socket::SocketLeaf>);
impl Leaf {
    pub fn from_listener(
        path: &std::path::Path,
        listener: &std::os::unix::net::UnixListener,
    ) -> Result<Self> {
        Ok(Self(Some(super::socket::SocketLeaf::from_listener(
            path, listener,
        )?)))
    }
}
impl Drop for Leaf {
    fn drop(&mut self) {
        if !cleanup_confirmed() {
            if let Some(leaf) = self.0.take() {
                std::mem::forget(leaf);
            }
        }
    }
}

/// Explicit fault-test barrier in this QA controller only. With the variables
/// absent, all normal source/native command and timing paths remain unchanged.
pub(super) fn hold(endpoint: &str, pid: u32, output: &std::path::Path) -> Result<()> {
    let Some(mode) = std::env::var_os("NATIVE_LOGGING_QA_HOLD") else {
        return Ok(());
    };
    if mode != "first" && mode != "runtime" {
        return Err("unsupported native logging QA hold mode".into());
    }
    if mode == "runtime" && endpoint != "runtime" {
        return Ok(());
    }
    let root = std::path::PathBuf::from(
        std::env::var_os("NATIVE_LOGGING_QA_RECORDS").ok_or("native logging QA records missing")?,
    );
    fs::create_dir_all(&root)?;
    if root.join("held.json").exists() {
        return Ok(());
    }
    fs::write(
        root.join("held.pending"),
        serde_json::to_vec(&serde_json::json!({"pid":pid,"endpoint":endpoint,"output":output}))?,
    )?;
    fs::rename(root.join("held.pending"), root.join("held.json"))?;
    let until = Instant::now() + Duration::from_secs(15);
    while !root.join("release").exists() {
        check()?;
        require(
            Instant::now() < until,
            "native logging fault barrier exceeded bound",
        )?;
        std::thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}
