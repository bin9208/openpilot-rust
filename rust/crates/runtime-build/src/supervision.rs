//! Dedicated native-build frontend/worker supervision. No runtime process joins
//! the worker's process group. The frontend survives worker failure; parent death
//! is independently observed by the worker, including a SIGKILL of the frontend.
use crate::{generation::offroad, Result};
use rustix::process::{
    getpgrp, getpid, getppid, kill_process_group, parent_process_death_signal,
    set_parent_process_death_signal, Pid, Signal,
};
use signal_hook::iterator::{Handle, Signals};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::{fs::OpenOptionsExt, process::CommandExt},
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

const PARENT: &str = "OPENPILOT_NATIVE_BUILD_PARENT";
const SCOPE_PARENT: &str = "OPENPILOT_NATIVE_SCOPE_PARENT";
pub const REQUEST_OWNER: &str = "OPENPILOT_NATIVE_REQUEST_OWNER";
const SIGNALS: [i32; 3] = [
    signal_hook::consts::SIGINT,
    signal_hook::consts::SIGTERM,
    signal_hook::consts::SIGHUP,
];

pub const OUTPUT_TAIL_BYTES: u64 = 64 * 1024;

/// Keep compiler/build-script output off the updater's captured-output pipe.
/// Both streams share one file description/offset in this owned attempt.
pub fn retain_output(command: &mut Command, path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(path)?;
    command
        .stdout(Stdio::from(file.try_clone()?))
        .stderr(Stdio::from(file.try_clone()?));
    Ok(file)
}

pub fn output_tail(path: &Path) -> Result<Vec<u8>> {
    let mut file = crate::regular_file(path)?;
    let length = file.metadata()?.len();
    file.seek(SeekFrom::Start(length.saturating_sub(OUTPUT_TAIL_BYTES)))?;
    let mut bytes = Vec::new();
    file.take(OUTPUT_TAIL_BYTES).read_to_end(&mut bytes)?;
    Ok(bytes)
}

struct ChildGroup {
    child: Child,
    group: Pid,
    reaped: bool,
}

struct FrontendParent {
    previous: Option<Signal>,
    request_owner: Option<String>,
}

/// Bind the caller to its Linux start time as well as PID. sudo may keep an
/// intermediate monitor alive after the original updater has already died.
pub fn process_identity(pid: u32) -> Result<String> {
    if pid <= 1 {
        return Err("invalid native build request owner".into());
    }
    // Keep the same kernel identity for self and peer readers under emulation.
    let stat = std::fs::read_to_string(format!("/proc/{pid}/task/{pid}/stat"))?;
    let fields: Vec<_> = stat
        .rsplit_once(") ")
        .ok_or("invalid process stat")?
        .1
        .split_whitespace()
        .collect();
    if matches!(fields.first(), Some(&"Z" | &"X")) {
        return Err("native build request owner has exited".into());
    }
    let start = fields
        .get(19)
        .ok_or("request owner start time missing")?
        .parse::<u64>()?;
    Ok(format!("{pid}:{start}"))
}

fn requested_owner() -> Result<Option<String>> {
    match std::env::var(REQUEST_OWNER) {
        Ok(identity) => Ok(Some(identity)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn verify_owner(identity: &str) -> Result<()> {
    // The original requester and each dedicated intermediary must stay alive.
    // A sudo monitor alone can survive either one's death. Keep the old single
    // PID:start form valid while bounding the new comma-separated owner chain.
    let mut seen = [None; 4];
    for (index, identity) in identity.split(',').enumerate() {
        if index >= seen.len() || seen[..index].contains(&Some(identity)) {
            return Err("invalid native build request owner chain".into());
        }
        seen[index] = Some(identity);
        let (pid, _) = identity
            .split_once(':')
            .ok_or("invalid request owner identity")?;
        if process_identity(pid.parse()?)? != identity {
            return Err("native build request owner identity changed".into());
        }
    }
    Ok(())
}

pub fn check_request_owner() -> Result<()> {
    if let Some(identity) = requested_owner()? {
        verify_owner(&identity)?;
    }
    Ok(())
}

impl FrontendParent {
    fn watch() -> Result<Self> {
        let parent = getppid().ok_or("native build frontend parent missing")?;
        if let Some(expected) = std::env::var_os(SCOPE_PARENT) {
            if expected.to_str().and_then(|s| s.parse::<i32>().ok())
                != Some(parent.as_raw_nonzero().get())
            {
                return Err("native scope parent died before frontend initialization".into());
            }
        }
        let guard = Self {
            previous: parent_process_death_signal()?,
            request_owner: requested_owner()?,
        };
        guard.check_request()?;
        set_parent_process_death_signal(Some(Signal::TERM))?;
        if getppid() != Some(parent) {
            return Err("native build frontend parent died during initialization".into());
        }
        Ok(guard)
    }

    fn check_request(&self) -> Result<()> {
        if let Some(identity) = &self.request_owner {
            verify_owner(identity)?;
        }
        Ok(())
    }
}

impl Drop for FrontendParent {
    fn drop(&mut self) {
        let _ = set_parent_process_death_signal(self.previous);
    }
}

impl Drop for ChildGroup {
    fn drop(&mut self) {
        // std::process::Command created precisely this child-owned group.
        // PID 1 must never reach kill(-1, ...), which means all permitted tasks.
        if !self.reaped && self.group != Pid::INIT {
            let _ = kill_process_group(self.group, Signal::KILL);
        }
        let _ = self.child.wait();
    }
}

/// Use only in a dedicated frontend process. command must enter WorkerGuard
/// before creating build children. stdout/stderr remain the worker's originals.
pub fn run(command: Command) -> Result<ExitStatus> {
    run_observed(command, || Ok(()))
}

pub fn run_observed(
    mut command: Command,
    mut observe: impl FnMut() -> Result<()>,
) -> Result<ExitStatus> {
    if getpid() == Pid::INIT {
        return Err("native build frontend cannot be PID 1".into());
    }
    let mut signals = Signals::new(SIGNALS)?;
    // Includes cancellation of a sudo/scope launcher. Install the signal
    // consumer before requesting parent-death delivery and spawning the worker.
    let parent = FrontendParent::watch()?;
    if signals.pending().next().is_some() {
        return Err("native build cancelled before worker startup".into());
    }
    let child = command
        .process_group(0)
        .env(PARENT, getpid().as_raw_nonzero().get().to_string())
        .stdin(Stdio::null())
        .spawn()?;
    let group =
        Pid::from_raw(i32::try_from(child.id())?).ok_or("invalid native build worker PID")?;
    if group == Pid::INIT {
        return Err("invalid native build worker process group".into());
    }
    let mut child = ChildGroup {
        child,
        group,
        reaped: false,
    };
    loop {
        parent.check_request()?;
        if signals.pending().next().is_some() {
            return Err("native build cancelled or frontend parent died".into());
        }
        observe()?;
        let exited = match rustix::process::waitid(
            rustix::process::WaitId::Pid(child.group),
            rustix::process::WaitIdOptions::EXITED
                | rustix::process::WaitIdOptions::NOHANG
                | rustix::process::WaitIdOptions::NOWAIT,
        ) {
            Ok(status) => status.is_some(),
            Err(rustix::io::Errno::INTR) => false,
            Err(error) => return Err(error.into()),
        };
        if exited {
            // Keep the leader waitable until its descendants are stopped: a
            // reaped PID could otherwise be reused by an unrelated process.
            let _ = kill_process_group(child.group, Signal::KILL);
            let status = child.child.wait()?;
            child.reaped = true;
            return Ok(status);
        }
        thread::sleep(Duration::from_millis(20));
    }
}

pub struct WorkerGuard {
    stopped: Arc<AtomicBool>,
    signals: Handle,
    threads: Vec<JoinHandle<()>>,
    previous_death_signal: Option<Signal>,
}

fn terminate_own_group(group: Pid, reason: &str) -> ! {
    if group == Pid::INIT || getpid() != group || getpgrp() != group {
        let _ = writeln!(
            std::io::stderr().lock(),
            "native build supervision refused an unowned process group"
        );
        std::process::exit(1);
    }
    // Cleanup must not depend on a writable log/pipe. eprintln! can panic on
    // ENOSPC/EPIPE and otherwise leave detached children alive after parent death.
    let cleanup = crate::memory_cgroup::stop_worker_children();
    let _ = writeln!(
        std::io::stderr().lock(),
        "native build stopped: {reason}; previous generation and logs retained"
    );
    if let Err(error) = cleanup {
        let _ = writeln!(
            std::io::stderr().lock(),
            "native build private memory group cleanup: {error}"
        );
    }
    let _ = kill_process_group(group, Signal::KILL);
    std::process::exit(1);
}

impl WorkerGuard {
    /// The private worker must be a group leader launched by the frontend. Never
    /// call this in the manager, updater, or a shared test process.
    pub fn start(params: &Path) -> Result<Self> {
        let group = getpid();
        if group == Pid::INIT || getpgrp() != group {
            return Err("native build worker needs its own process group".into());
        }
        let parent = std::env::var(PARENT)?.parse::<i32>()?;
        let parent = Pid::from_raw(parent).ok_or("invalid native build parent")?;
        if getppid() != Some(parent) {
            return Err("native build parent handshake mismatch".into());
        }
        offroad(params)?;
        let previous_death_signal = parent_process_death_signal()?;
        let mut signals = Signals::new(SIGNALS)?;
        let handle = signals.handle();
        set_parent_process_death_signal(Some(Signal::TERM))?;
        if getppid() != Some(parent) {
            terminate_own_group(group, "frontend died during worker initialization");
        }
        let stopped = Arc::new(AtomicBool::new(false));
        let mut guard = Self {
            stopped: Arc::clone(&stopped),
            signals: handle,
            threads: Vec::new(),
            previous_death_signal,
        };
        let signal_stop = Arc::clone(&stopped);
        guard.threads.push(
            thread::Builder::new()
                .name("native-build-signals".into())
                .spawn(move || {
                    for _ in signals.forever() {
                        if !signal_stop.load(Ordering::Acquire) {
                            terminate_own_group(group, "signal or frontend death");
                        }
                    }
                })?,
        );
        let params = params.to_owned();
        guard.threads.push(
            thread::Builder::new()
                .name("native-build-offroad".into())
                .spawn(move || {
                    while !stopped.load(Ordering::Acquire) {
                        if offroad(&params).is_err() {
                            terminate_own_group(
                                group,
                                "offroad state changed or became unreadable",
                            );
                        }
                        thread::sleep(Duration::from_millis(200));
                    }
                })?,
        );
        Ok(guard)
    }
}

impl Drop for WorkerGuard {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        self.signals.close();
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
        let _ = set_parent_process_death_signal(self.previous_death_signal);
    }
}
