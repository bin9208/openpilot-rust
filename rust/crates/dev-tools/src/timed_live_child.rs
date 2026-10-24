//! Interactive QA child whose unreaped leader reserves its process-group ID.
use crate::{qa_process::CommandSpec, seal::Result};
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
    process::{kill_process, kill_process_group, waitid, Pid, Signal, WaitId, WaitIdOptions},
};
use std::{
    fs::File,
    io::{self, Write},
    os::unix::process::{CommandExt, ExitStatusExt},
    path::Path,
    process::{Child as Process, Command, Stdio},
    time::{Duration, Instant},
};

pub struct Child {
    process: Process,
    pid: Pid,
    reaped: Option<i32>,
}
impl Child {
    pub fn spawn(spec: &CommandSpec, output: &Path) -> Result<Self> {
        let argv = spec.effective_argv();
        let executable = argv.first().ok_or("empty timed child command")?;
        // Original controllers open their shared stdout/stderr file before
        // spawning. A failed executable therefore still leaves that empty file.
        let file = File::create(output)?;
        let mut command = Command::new(executable);
        command
            .args(&argv[1..])
            .envs(spec.effective_env())
            .process_group(0)
            .stdout(Stdio::from(file.try_clone()?))
            .stderr(Stdio::from(file));
        for key in &spec.env_remove {
            command.env_remove(key);
        }
        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }
        if spec.disable_core_dump {
            openpilot_process_limits::disable_core_dump(&mut command);
        }
        command.stdin(if spec.input.is_some() {
            Stdio::piped()
        } else if spec.inherit_stdin {
            Stdio::inherit()
        } else {
            Stdio::null()
        });
        let process = command.spawn()?;
        let pid = Pid::from_child(&process);
        let mut owner = Self {
            process,
            pid,
            reaped: None,
        };
        if let Some(input) = &spec.input {
            let mut pipe = owner
                .process
                .stdin
                .take()
                .ok_or("missing timed child stdin")?;
            fcntl_setfl(&pipe, fcntl_getfl(&pipe)? | OFlags::NONBLOCK)?;
            let deadline = Instant::now() + spec.timeout;
            let mut remaining = input.as_slice();
            while !remaining.is_empty() {
                match pipe.write(remaining) {
                    Ok(0) => return Err("timed child stdin closed".into()),
                    Ok(count) => remaining = &remaining[count..],
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        let duration = deadline.saturating_duration_since(Instant::now());
                        if duration.is_zero() {
                            return Err("timed child stdin timeout".into());
                        }
                        let timeout = Timespec::try_from(duration)?;
                        let mut descriptors = [PollFd::new(&pipe, PollFlags::OUT)];
                        match poll(&mut descriptors, Some(&timeout)) {
                            Ok(0) => return Err("timed child stdin timeout".into()),
                            Ok(_) | Err(rustix::io::Errno::INTR) => {}
                            Err(error) => return Err(error.into()),
                        }
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            // Close before waiting for IPC readiness, as in original Popen.
        }
        Ok(owner)
    }
    pub fn id(&self) -> u32 {
        self.pid
            .as_raw_nonzero()
            .get()
            .try_into()
            .expect("positive owned PID")
    }
    pub fn poll(&self) -> Result<Option<i32>> {
        if let Some(code) = self.reaped {
            return Ok(Some(code));
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
        if self.reaped.is_none() {
            match kill_process_group(self.pid, Signal::KILL) {
                Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
    pub fn wait(&mut self, timeout: Duration) -> Result<i32> {
        if let Some(code) = self.reaped {
            return Ok(code);
        }
        let deadline = Instant::now() + timeout;
        while self.poll()?.is_none() {
            if Instant::now() >= deadline {
                return Err("timed child wait timeout".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        // Stop owned descendants while the exited direct child still reserves
        // the numeric group ID; only then may wait reap and release it.
        self.kill()?;
        let status = self.process.wait()?;
        let code = status
            .code()
            .unwrap_or_else(|| -status.signal().unwrap_or(1));
        self.reaped = Some(code);
        Ok(code)
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        if self.reaped.is_none() {
            let _ = self.kill();
            let _ = self.process.kill();
            let _ = self.process.wait();
        }
    }
}
