use super::shell;
use crate::{config::Config, Error};
use openpilot_process_supervision::{pty, CapturedChild, CapturedCommand};
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    process::{Pid, Signal, WaitId, WaitIdOptions, WaitIdStatus},
};
use std::{
    fs::File,
    time::{Duration, Instant},
};

#[cfg(test)]
#[path = "pty_tests.rs"]
mod tests;

pub(super) struct Generation {
    pub master: File,
    child: CapturedChild,
    pub pid: Pid,
}
impl Generation {
    pub fn start(config: &Config) -> Result<Self, Error> {
        let pair = pty::Pair::open(30, 100)?;
        let flags = rustix::fs::fcntl_getfl(&pair.master).map_err(std::io::Error::from)?;
        rustix::fs::fcntl_setfl(&pair.master, flags | rustix::fs::OFlags::NONBLOCK)
            .map_err(std::io::Error::from)?;
        let command = shell::command(config)?;
        let executable = std::env::var_os("SHELL")
            .filter(|shell| !shell.is_empty())
            .unwrap_or_else(|| "/bin/bash".into());
        let child = CapturedCommand {
            launcher: config.launcher.clone(),
            cwd: shell::cwd(config),
            argv: vec![executable, "-lc".into(), command.into()],
        }
        .spawn_session_pty_with_env(pair.slave, &shell::environment())?;
        let pid = i32::try_from(child.process.id())
            .ok()
            .and_then(Pid::from_raw)
            .ok_or_else(|| Error::Source("PTY process ID exceeds OS range".into()))?;
        Ok(Self {
            master: pair.master,
            child,
            pid,
        })
    }
    pub fn observe(&self) -> Result<Option<WaitIdStatus>, Error> {
        // NOWAIT keeps the leader identity reserved until all group signals finish.
        loop {
            match rustix::process::waitid(
                WaitId::Pid(self.pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            ) {
                Ok(status) => return Ok(status),
                Err(rustix::io::Errno::INTR) => {}
                Err(error) => return Err(std::io::Error::from(error).into()),
            }
        }
    }
    pub fn observe_eof(&self) -> Result<Option<WaitIdStatus>, Error> {
        let deadline = Instant::now() + Duration::from_millis(50);
        loop {
            let observed = self.observe()?;
            if observed.is_some() || Instant::now() >= deadline {
                return Ok(observed);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    pub fn signal(&self, signal: Signal) -> Result<(), Error> {
        match rustix::process::kill_process_group(self.pid, signal) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
            Err(error) => Err(std::io::Error::from(error).into()),
        }
    }
    pub fn ready(&self) -> Result<bool, Error> {
        let mut descriptors = [PollFd::new(&self.master, PollFlags::IN)];
        match poll(
            &mut descriptors,
            Some(&Timespec {
                tv_sec: 0,
                tv_nsec: 10_000_000,
            }),
        ) {
            Ok(count) => Ok(count > 0),
            Err(rustix::io::Errno::INTR) => Ok(false),
            Err(error) => Err(std::io::Error::from(error).into()),
        }
    }
    pub fn write(&self, mut bytes: &[u8]) -> Result<(), Error> {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !bytes.is_empty() {
            match rustix::io::write(&self.master, bytes) {
                Ok(0) => return Err(Error::Source("pty write failed".into())),
                Ok(count) => bytes = &bytes[count..],
                Err(rustix::io::Errno::INTR) => {}
                Err(rustix::io::Errno::AGAIN) => {
                    if Instant::now() >= deadline {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "pty write timed out",
                        )
                        .into());
                    }
                    let mut descriptors = [PollFd::new(&self.master, PollFlags::OUT)];
                    match poll(
                        &mut descriptors,
                        Some(&Timespec {
                            tv_sec: 0,
                            tv_nsec: 10_000_000,
                        }),
                    ) {
                        Ok(_) | Err(rustix::io::Errno::INTR) => {}
                        Err(error) => return Err(std::io::Error::from(error).into()),
                    }
                }
                Err(error) => return Err(std::io::Error::from(error).into()),
            }
        }
        Ok(())
    }
}
pub(super) fn exit_code(status: WaitIdStatus) -> i32 {
    status
        .exit_status()
        .unwrap_or_else(|| -status.terminating_signal().unwrap_or(0))
}
impl Drop for Generation {
    fn drop(&mut self) {
        // Native final ownership includes descendants even after leader exit.
        for signal in [Signal::HUP, Signal::KILL] {
            if let Err(error) = self.signal(signal) {
                eprintln!("[recovery] PTY group close: {error}");
            }
        }
        if let Err(error) = super::session::cleanup(self.pid) {
            eprintln!("[recovery] PTY session descendants: {error}");
        }
        if let Err(error) = self.child.process.wait() {
            eprintln!("[recovery] PTY child reap: {error}");
        }
    }
}
