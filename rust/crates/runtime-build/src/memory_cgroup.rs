//! A private memory cgroup for one native build. Scope setup changes only the
//! application's own empty parent and moves only the new setup/frontend process.
//! Shared controller settings and existing runtime/runner processes stay intact.
use crate::Result;
use rustix::process::{
    getpid, getppid, kill_process, pidfd_open, pidfd_send_signal, Gid, Pid, PidfdFlags, Signal, Uid,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

pub const ENV: &str = "OPENPILOT_NATIVE_BUILD_MEMORY_GROUP";
static ENTERED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Version {
    V1,
    V2,
}

#[derive(Clone, Debug, Serialize)]
pub struct Memory {
    pub controller: Version,
    pub current_bytes: u64,
    /// Kernel cgroup charge high-water mark, including descendants and cache.
    pub kernel_peak_bytes: u64,
    /// v1 failcnt includes rejected/reclaimed allocations, not only OOM kills.
    pub limit_events: u64,
}

pub struct Group {
    pub path: PathBuf,
    version: Version,
    owned: bool,
}

fn controller(path: &Path) -> Result<Version> {
    if path.canonicalize()? != path || !fs::symlink_metadata(path)?.is_dir() {
        return Err("memory cgroup path must be a real canonical directory".into());
    }
    match rustix::fs::fstatfs(File::open(path)?)?.f_type as u64 {
        0x6367_7270 => Ok(Version::V2),
        0x0027_e0eb => Ok(Version::V1),
        _ => Err(
            "native build requires a delegated kernel memory cgroup, not an ordinary directory"
                .into(),
        ),
    }
}

fn number(path: &Path) -> Result<u64> {
    Ok(fs::read_to_string(path)?.trim().parse()?)
}

fn control(root: &Path, file: &str, value: impl ToString) -> Result<()> {
    let path = root.join(file);
    // Never create a normal file to imitate a missing kernel controller.
    if !fs::symlink_metadata(&path)?.is_file() {
        return Err("memory controller file is redirected".into());
    }
    Ok(fs::write(path, value.to_string())?)
}

fn active_swap() -> Result<bool> {
    Ok(fs::read_to_string("/proc/swaps")?
        .lines()
        .skip(1)
        .any(|s| !s.trim().is_empty()))
}

impl Group {
    pub fn create(parent: &Path, limit: u64) -> Result<Self> {
        if limit == 0 {
            return Err("native build memory limit must be explicit and positive".into());
        }
        let version = controller(parent)?;
        let path = tempfile::Builder::new()
            .prefix(&format!("build-{}-", std::process::id()))
            .tempdir_in(parent)?
            .keep();
        let group = Self {
            path,
            version,
            owned: true,
        };
        match version {
            Version::V2 => {
                control(&group.path, "memory.max", limit)?;
                control(&group.path, "memory.oom.group", 1)?;
                if group.path.join("memory.swap.max").exists() {
                    control(&group.path, "memory.swap.max", 0)?;
                } else if active_swap()? {
                    return Err("memory cgroup cannot disable active swap".into());
                }
            }
            Version::V1 => {
                control(&group.path, "memory.use_hierarchy", 1)?;
                control(&group.path, "memory.limit_in_bytes", limit)?;
                if group.path.join("memory.memsw.limit_in_bytes").exists() {
                    control(&group.path, "memory.memsw.limit_in_bytes", limit)?;
                } else if active_swap()? {
                    return Err("memory cgroup has no active swap accounting".into());
                }
                control(&group.path, "memory.oom_control", 0)?;
                if group.path.join("memory.swappiness").exists() {
                    control(&group.path, "memory.swappiness", 0)?;
                }
            }
        }
        group.verify_limit(limit)?;
        group.memory()?; // A real kernel peak counter is mandatory.
        Ok(group)
    }

    fn verify_limit(&self, maximum: u64) -> Result<()> {
        let actual = number(&self.path.join(match self.version {
            Version::V1 => "memory.limit_in_bytes",
            Version::V2 => "memory.max",
        }))?;
        if actual == 0 || actual > maximum {
            return Err("kernel memory limit exceeds the configured build budget".into());
        }
        match self.version {
            Version::V1 => {
                if number(&self.path.join("memory.use_hierarchy"))? != 1 {
                    return Err("build memory hierarchy is disabled".into());
                }
                let path = self.path.join("memory.memsw.limit_in_bytes");
                if path.exists() && number(&path)? > maximum {
                    return Err("build memory+swap limit exceeds budget".into());
                }
                if fs::read_to_string(self.path.join("memory.oom_control"))?
                    .lines()
                    .any(|line| line == "oom_kill_disable 1")
                {
                    return Err("build cgroup OOM killing is disabled".into());
                }
            }
            Version::V2 => {
                if number(&self.path.join("memory.oom.group"))? != 1 {
                    return Err("build OOM group policy changed".into());
                }
                let path = self.path.join("memory.swap.max");
                if path.exists() && number(&path)? != 0 {
                    return Err("build swap policy changed".into());
                }
            }
        }
        Ok(())
    }

    pub fn memory(&self) -> Result<Memory> {
        let (current, peak, events) = match self.version {
            Version::V1 => (
                "memory.usage_in_bytes",
                "memory.max_usage_in_bytes",
                number(&self.path.join("memory.failcnt"))?,
            ),
            Version::V2 => {
                let events = fs::read_to_string(self.path.join("memory.events"))?;
                let count = events
                    .lines()
                    .filter_map(|line| line.split_once(' '))
                    .filter(|(key, _)| matches!(*key, "max" | "oom" | "oom_kill"))
                    .try_fold(0_u64, |sum, (_, value)| -> Result<u64> {
                        Ok(sum
                            .checked_add(value.parse()?)
                            .ok_or("memory event overflow")?)
                    })?;
                ("memory.current", "memory.peak", count)
            }
        };
        Ok(Memory {
            controller: self.version,
            current_bytes: number(&self.path.join(current))?,
            kernel_peak_bytes: number(&self.path.join(peak))?,
            limit_events: events,
        })
    }

    pub fn check(&self, maximum: u64) -> Result<()> {
        self.verify_limit(maximum)
    }

    pub fn stop(&self) -> Result<()> {
        stop(&self.path, self.version, None)
    }
}

impl Drop for Group {
    fn drop(&mut self) {
        if !self.owned {
            return;
        }
        if let Err(error) = self.stop().and_then(|()| Ok(fs::remove_dir(&self.path)?)) {
            eprintln!("native build private cgroup retained for recovery: {error}");
        }
    }
}

/// The worker joins before spawning Cargo or any compiler/model-preparation
/// children. Membership is inherited even if a child creates a new session.
pub fn enter(parent: &Path, maximum: u64) -> Result<()> {
    let path = PathBuf::from(std::env::var_os(ENV).ok_or("native build memory group missing")?);
    let expected = format!(
        "build-{}-",
        getppid()
            .ok_or("native build parent missing")?
            .as_raw_nonzero()
    );
    if path.parent() != Some(parent)
        || !path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.starts_with(&expected))
    {
        return Err("native worker memory group does not belong to its frontend".into());
    }
    let version = controller(&path)?;
    // This borrowed handle must not remove the frontend-owned directory.
    let group = Group {
        path,
        version,
        owned: false,
    };
    group.verify_limit(maximum)?;
    control(&group.path, "cgroup.procs", getpid().as_raw_nonzero())?;
    if !members(&group.path)?.contains(&std::process::id()) {
        return Err("native worker did not enter its private memory group".into());
    }
    ENTERED.store(true, Ordering::Release);
    Ok(())
}

fn members(path: &Path) -> Result<Vec<u32>> {
    fs::read_to_string(path.join("cgroup.procs"))?
        .split_whitespace()
        .map(|value| Ok(value.parse()?))
        .collect()
}

fn directories(root: &Path) -> Result<Vec<PathBuf>> {
    let mut directories = vec![root.to_owned()];
    let mut index = 0;
    while index < directories.len() {
        for entry in fs::read_dir(&directories[index])? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if directories.len() >= 1024 {
                    return Err("excessive build cgroup descendants".into());
                }
                directories.push(entry.path());
            }
        }
        index += 1;
    }
    Ok(directories)
}

fn kill_member(path: &Path, raw: u32) -> Result<()> {
    let pid = Pid::from_raw(i32::try_from(raw)?)
        .filter(|pid| *pid != Pid::INIT)
        .ok_or("unsafe build cgroup PID")?;
    match pidfd_open(pid, PidfdFlags::empty()) {
        Ok(fd) => {
            if members(path)?.contains(&raw) {
                match pidfd_send_signal(fd, Signal::KILL) {
                    Ok(()) | Err(rustix::io::Errno::SRCH) => (),
                    Err(e) => return Err(e.into()),
                }
            }
        }
        // Older AGNOS kernels do not expose pidfds. Recheck membership directly
        // before signaling; the caller only ever enumerates its owned cgroup.
        Err(rustix::io::Errno::NOSYS | rustix::io::Errno::INVAL) => {
            if members(path)?.contains(&raw) {
                match kill_process(pid, Signal::KILL) {
                    Ok(()) | Err(rustix::io::Errno::SRCH) => (),
                    Err(e) => return Err(e.into()),
                }
            }
        }
        Err(rustix::io::Errno::SRCH) => (),
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

fn stop(path: &Path, version: Version, keep: Option<u32>) -> Result<()> {
    controller(path)?;
    if keep.is_none() && matches!(version, Version::V2) && path.join("cgroup.kill").exists() {
        control(path, "cgroup.kill", 1)?;
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let directories = directories(path)?;
        let mut remaining = false;
        for directory in &directories {
            for pid in members(directory)?
                .into_iter()
                .filter(|pid| Some(*pid) != keep)
            {
                remaining = true;
                kill_member(directory, pid)?;
            }
        }
        if !remaining {
            for directory in directories.iter().skip(1).rev() {
                fs::remove_dir(directory)?;
            }
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("owned build cgroup did not become empty".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
}

/// Called by the worker's parent-death/signal path before it kills itself. The
/// memory group contains the worker and descendants, never the normal runtime.
pub(crate) fn stop_worker_children() -> Result<()> {
    if !ENTERED.load(Ordering::Acquire) {
        return Ok(());
    }
    let Some(path) = std::env::var_os(ENV) else {
        return Ok(());
    };
    let path = PathBuf::from(path);
    let version = controller(&path)?;
    if !members(&path)?.contains(&std::process::id()) {
        return Err("worker is outside its assigned memory group".into());
    }
    stop(&path, version, Some(std::process::id()))
}

/// Set up only this application's fixed per-user scope. The shared controller
/// must already expose memory; this function never enables it globally.
pub fn prepare_scope(parent: &Path, uid: u32, gid: u32) -> Result<PathBuf> {
    if !rustix::process::geteuid().is_root() || uid == 0 || uid == u32::MAX || gid == u32::MAX {
        return Err("build scope setup needs root and a non-root sudo caller".into());
    }
    let (root, version) = match controller(Path::new("/sys/fs/cgroup")) {
        Ok(Version::V2) => (PathBuf::from("/sys/fs/cgroup"), Version::V2),
        _ => {
            let root = PathBuf::from("/sys/fs/cgroup/memory");
            (root.clone(), controller(&root)?)
        }
    };
    if parent != root.join(format!("openpilot-native-build-{uid}")) {
        return Err("refusing a foreign memory scope path".into());
    }
    if matches!(version, Version::V2)
        && !fs::read_to_string(root.join("cgroup.subtree_control"))?
            .split_whitespace()
            .any(|s| s == "memory")
    {
        return Err(
            "memory controller is not delegated at this root; shared controller left unchanged"
                .into(),
        );
    }
    match fs::create_dir(parent) {
        Ok(()) => rustix::fs::chown(parent, Some(Uid::from_raw(uid)), Some(Gid::from_raw(gid)))?,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    controller(parent)?;
    if fs::metadata(parent)?.uid() != uid || !members(parent)?.is_empty() {
        return Err("native scope belongs to another user or contains unexpected processes".into());
    }
    match version {
        Version::V2 => {
            if !fs::read_to_string(parent.join("cgroup.subtree_control"))?
                .split_whitespace()
                .any(|s| s == "memory")
            {
                if fs::read_dir(parent)?
                    .filter_map(|e| e.ok())
                    .any(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                {
                    return Err("refusing to change an in-use native scope controller".into());
                }
                control(parent, "cgroup.subtree_control", "+memory")?;
            }
        }
        Version::V1 => {
            if number(&parent.join("memory.use_hierarchy"))? != 1 {
                if fs::read_dir(parent)?.any(|entry| {
                    entry.is_ok_and(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                }) {
                    return Err("refusing to change an in-use native memory hierarchy".into());
                }
                control(parent, "memory.use_hierarchy", 1)?;
            }
        }
    }
    let control_group = parent.join("control");
    match fs::create_dir(&control_group) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    controller(&control_group)?;
    for path in [
        parent.to_owned(),
        parent.join("cgroup.procs"),
        control_group.clone(),
        control_group.join("cgroup.procs"),
    ] {
        rustix::fs::chown(&path, Some(Uid::from_raw(uid)), Some(Gid::from_raw(gid)))?;
    }
    Ok(control_group)
}

/// Only the new scope helper/frontend is moved; no existing runtime process is.
pub fn enter_control(control_group: &Path) -> Result<()> {
    controller(control_group)?;
    control(control_group, "cgroup.procs", std::process::id())
}

pub fn verify_control(parent: &Path) -> Result<()> {
    let uid = rustix::process::getuid();
    if uid.is_root() || rustix::process::geteuid() != uid {
        return Err("native build frontend must run as its original non-root user".into());
    }
    let control_group = parent.join("control");
    controller(&control_group)?;
    if !members(&control_group)?.contains(&std::process::id()) {
        return Err("native build frontend is outside its delegated control scope".into());
    }
    Ok(())
}
