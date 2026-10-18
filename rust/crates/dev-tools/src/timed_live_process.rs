//! Direct child of an already-owned worker group. Do not create a nested group:
//! the outer QA runner must also terminate this daemon if the worker is killed.
use crate::{qa_process::CommandSpec, seal::Result};
use rustix::process::{kill_process, waitid, Pid, Signal, WaitId, WaitIdOptions};
use std::{
    fs::File,
    io::Write,
    os::unix::process::ExitStatusExt,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
pub(super) struct Process {
    child: Child,
    pid: Pid,
    status: Option<i32>,
}
impl Process {
    pub fn spawn<T>(
        spec: &CommandSpec,
        path: &Path,
        prepare_metadata: impl FnOnce() -> Result<T>,
    ) -> Result<(Self, T)> {
        let argv = spec.effective_argv();
        let program = argv.first().ok_or("empty live timed command")?;
        let file = File::create(path)?;
        let mut command = Command::new(crate::timed_fixtures::lexical(Path::new(program)));
        command
            .args(&argv[1..])
            .envs(spec.effective_env())
            .stdin(if spec.inherit_stdin {
                Stdio::inherit()
            } else {
                Stdio::piped()
            })
            .stdout(Stdio::from(file.try_clone()?))
            .stderr(Stdio::from(file));
        for key in &spec.env_remove {
            command.env_remove(key);
        }
        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }
        // Open the original log first, but finish potentially slow provenance
        // reads before the child can publish clocks or start an HTTP request.
        let metadata = prepare_metadata()?;
        let child = command.spawn()?;
        let pid = Pid::from_child(&child);
        let mut owner = Self {
            child,
            pid,
            status: None,
        };
        if let Some(input) = &spec.input {
            owner
                .child
                .stdin
                .take()
                .ok_or("missing live stdin")?
                .write_all(input)?;
        } else {
            owner.child.stdin.take();
        }
        Ok((owner, metadata))
    }
    pub fn id(&self) -> u32 {
        self.pid.as_raw_nonzero().get() as u32
    }
    pub fn poll(&self) -> Result<Option<i32>> {
        if let Some(status) = self.status {
            return Ok(Some(status));
        }
        loop {
            match waitid(
                WaitId::Pid(self.pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            ) {
                Ok(status) => {
                    return Ok(status.map(|status| {
                        status
                            .exit_status()
                            .unwrap_or_else(|| -status.terminating_signal().unwrap_or(1))
                    }))
                }
                Err(rustix::io::Errno::INTR) => continue,
                Err(error) => return Err(error.into()),
            }
        }
    }
    pub fn signal(&self, signal: Signal) -> Result<()> {
        if self.poll()?.is_none() {
            match kill_process(self.pid, signal) {
                Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
    pub fn kill(&self) -> Result<()> {
        self.signal(Signal::KILL)
    }
    pub fn wait(&mut self, timeout: Duration) -> Result<i32> {
        if let Some(status) = self.status {
            return Ok(status);
        }
        let end = Instant::now() + timeout;
        while self.poll()?.is_none() {
            if Instant::now() >= end {
                return Err("timed live child wait timeout".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let status = self.child.wait()?;
        let code = status
            .code()
            .unwrap_or_else(|| -status.signal().unwrap_or(1));
        self.status = Some(code);
        Ok(code)
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if self.status.is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
