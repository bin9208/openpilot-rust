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
    os::unix::process::CommandExt,
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
const SIGNALS: [i32; 3] = [
    signal_hook::consts::SIGINT,
    signal_hook::consts::SIGTERM,
    signal_hook::consts::SIGHUP,
];

struct ChildGroup {
    child: Child,
    group: Pid,
    reaped: bool,
}

struct FrontendParent {
    previous: Option<Signal>,
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
        };
        set_parent_process_death_signal(Some(Signal::TERM))?;
        if getppid() != Some(parent) {
            return Err("native build frontend parent died during initialization".into());
        }
        Ok(guard)
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
    let _parent = FrontendParent::watch()?;
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
        eprintln!("native build supervision refused an unowned process group");
        std::process::exit(1);
    }
    eprintln!("native build stopped: {reason}; previous generation and logs retained");
    if let Err(error) = crate::memory_cgroup::stop_worker_children() {
        eprintln!("native build private memory group cleanup: {error}");
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
