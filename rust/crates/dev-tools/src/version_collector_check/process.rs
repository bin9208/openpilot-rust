//! Direct collector child in the worker's already-owned group, never a new group.
use crate::seal::Result;
use rustix::process::{waitid, Pid, Signal, WaitId, WaitIdOptions};
use std::{
    os::unix::process::ExitStatusExt,
    process::{Child, Command},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub(super) struct Process {
    child: Child,
    pid: Pid,
    reaped: bool,
}
pub(super) fn interrupted(stop: &AtomicBool) -> Result<()> {
    if stop.load(Ordering::Relaxed) {
        Err("Version collector interrupted".into())
    } else {
        Ok(())
    }
}
impl Process {
    pub fn spawn(command: &mut Command) -> Result<Self> {
        let child = command.spawn()?;
        let pid = Pid::from_child(&child);
        // Establish ownership before any later fallible queue or pipe setup.
        Ok(Self {
            child,
            pid,
            reaped: false,
        })
    }
    pub fn exited(&self) -> Result<bool> {
        match waitid(
            WaitId::Pid(self.pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        ) {
            Ok(status) => Ok(status.is_some()),
            Err(rustix::io::Errno::INTR) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
    pub fn signal(&self, signal: Signal) -> Result<()> {
        // The child has not been reaped, so its numeric identity is reserved.
        match rustix::process::kill_process(self.pid, signal) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
    pub fn wait(&mut self, timeout: Duration, stop: &AtomicBool) -> Result<i32> {
        let start = Instant::now();
        loop {
            interrupted(stop)?;
            if self.exited()? {
                let status = self.child.wait()?;
                self.reaped = true;
                return Ok(status
                    .code()
                    .unwrap_or_else(|| -status.signal().unwrap_or(1)));
            }
            if start.elapsed() >= timeout {
                return Err(format!("Version collector did not exit within {timeout:?}").into());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if !self.reaped {
            // Signal only this still-owned Child, never a released PID/reported
            // group. The outer runner separately terminates descendants.
            let _ = self.child.kill();
            let _ = self.child.wait();
            self.reaped = true;
        }
    }
}
