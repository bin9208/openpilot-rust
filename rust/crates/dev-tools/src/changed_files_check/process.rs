//! Git runs in owned process groups through the existing bounded QA runner.
//! The runner retains its unreaped direct leader until group cleanup, preventing
//! group-ID reuse while killing descendants. Non-child zombies are not reaped.
use super::{Diagnostic, Phase};
use crate::qa_process::{CaptureError, CommandSpec, NativeRunner};
use std::{
    ffi::{OsStr, OsString},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
#[derive(Clone)]
pub struct RunControl {
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
}
impl RunControl {
    pub fn new(timeout: Duration) -> Result<Self, Diagnostic> {
        if timeout.is_zero() {
            return Err(Diagnostic::new(Phase::Git, "Git deadline must be positive"));
        }
        Ok(Self {
            deadline: Instant::now()
                .checked_add(timeout)
                .ok_or_else(|| Diagnostic::new(Phase::Git, "Git deadline overflow"))?,
            cancelled: Arc::new(AtomicBool::new(false)),
        })
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
    pub(super) fn check(&self) -> Result<(), Diagnostic> {
        if self.cancelled.load(Ordering::Relaxed) {
            return Err(Diagnostic::new(
                Phase::Cancelled,
                "changed-file check cancelled",
            ));
        }
        if Instant::now() >= self.deadline {
            return Err(Diagnostic::new(
                Phase::Timeout,
                "changed-file check deadline expired",
            ));
        }
        Ok(())
    }
}
pub(super) struct Signals {
    ids: Vec<signal_hook::SigId>,
}
impl Signals {
    pub(super) fn install(control: &RunControl) -> Result<Self, std::io::Error> {
        let mut result = Self { ids: vec![] };
        for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            result.ids.push(signal_hook::flag::register(
                signal,
                Arc::clone(&control.cancelled),
            )?);
        }
        Ok(result)
    }
}
impl Drop for Signals {
    fn drop(&mut self) {
        for id in self.ids.drain(..) {
            signal_hook::low_level::unregister(id);
        }
    }
}
pub(super) fn git(
    root: &Path,
    args: &[&OsStr],
    inherit: bool,
    phase: Phase,
    control: &RunControl,
) -> Result<Vec<u8>, Diagnostic> {
    control.check()?;
    let argv = std::iter::once(OsString::from("git"))
        .chain(args.iter().map(|arg| arg.to_os_string()))
        .collect();
    let mut spec = CommandSpec::new_os(argv, Duration::from_secs(120));
    spec.cwd = Some(root.to_owned());
    spec.inherit_output = inherit;
    spec.inherit_stdin = inherit;
    spec.stdout_limit = 64 * 1024 * 1024;
    spec.stderr_limit = 1024 * 1024;
    let result = NativeRunner
        .run_controlled(&spec, control.deadline, &control.cancelled)
        .map_err(|error| {
            let phase = if control.cancelled.load(Ordering::Relaxed)
                || error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::Interrupted)
            {
                Phase::Cancelled
            } else if error
                .downcast_ref::<CaptureError>()
                .is_some_and(|e| matches!(e, CaptureError::Timeout { .. }))
            {
                Phase::Timeout
            } else {
                Phase::Git
            };
            Diagnostic::new(phase, format!("owned Git command failed: {error}"))
        })?;
    if result.code != 0 {
        return Err(Diagnostic::new(
            phase,
            format!(
                "git {:?} exited {}: {}",
                args,
                result.code,
                String::from_utf8_lossy(&result.stderr)
            ),
        ));
    }
    Ok(result.stdout)
}
