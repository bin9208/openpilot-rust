use super::Result;
use rustix::process::{kill_process_group, waitid, Pid, Signal, WaitId, WaitIdOptions};
use std::{
    ffi::OsString,
    fs::File,
    io::Write,
    os::unix::process::CommandExt,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

/// An owned recovery server with a private process group. Keeping the leader
/// unreaped until teardown keeps that group's numeric ID reserved.
pub struct OwnedServer {
    child: Child,
    reaped: bool,
}

impl OwnedServer {
    pub fn spawn(
        argv: &[OsString],
        repository: &Path,
        log: &Path,
        environment: &[(OsString, OsString)],
        remove_environment: &[&str],
        piped_stdin: bool,
    ) -> Result<Self> {
        let executable = argv.first().ok_or("empty recovery server argv")?;
        let log = File::create_new(log)?;
        let mut command = Command::new(executable);
        command
            .args(&argv[1..])
            .current_dir(repository)
            .env("OPENPILOT_ROOT", repository)
            .envs(environment.iter().cloned())
            .process_group(0)
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log))
            .stdin(if piped_stdin {
                Stdio::piped()
            } else {
                Stdio::null()
            });
        for key in remove_environment {
            command.env_remove(key);
        }
        Ok(Self {
            child: command.spawn()?,
            reaped: false,
        })
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    pub fn source_eof(&mut self) -> Result<()> {
        let mut input = self
            .child
            .stdin
            .take()
            .ok_or("missing source recovery stdin")?;
        input.write_all(b"\n")?;
        Ok(())
    }

    pub fn signal(&self, signal: Signal) -> Result<()> {
        let pid = Pid::from_child(&self.child);
        match rustix::process::kill_process(pid, signal) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn finish(&mut self, timeout: Duration) -> Result<i32> {
        if self.reaped {
            return Err("recovery child already reaped".into());
        }
        self.wait_unreaped(timeout)?;
        let pid = Pid::from_child(&self.child);
        // The unreaped leader still owns this process-group number. End any
        // descendants before releasing that identity, including on success.
        match kill_process_group(pid, Signal::KILL) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => {}
            Err(error) => return Err(error.into()),
        }
        let status = self.child.wait()?;
        self.reaped = true;
        Ok(status.code().unwrap_or(-1))
    }

    /// Observe leader exit without reaping or cleaning descendants. Required
    /// before assertions about the candidate's own child cleanup.
    pub fn wait_unreaped(&self, timeout: Duration) -> Result<i32> {
        if self.reaped {
            return Err("recovery child already reaped".into());
        }
        let end = Instant::now() + timeout;
        let pid = Pid::from_child(&self.child);
        loop {
            match waitid(
                WaitId::Pid(pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            ) {
                Ok(Some(status)) => {
                    return Ok(status
                        .exit_status()
                        .unwrap_or_else(|| -status.terminating_signal().unwrap_or(1)));
                }
                Ok(None) | Err(rustix::io::Errno::INTR) => {}
                Err(error) => return Err(error.into()),
            }
            if Instant::now() >= end {
                return Err("recovery server shutdown timeout".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for OwnedServer {
    fn drop(&mut self) {
        if self.reaped {
            return;
        }
        let pid = Pid::from_child(&self.child);
        let _ = kill_process_group(pid, Signal::KILL);
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
