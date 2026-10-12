use super::git::{GitCommands, GitOutput};
use rustix::process::{kill_process_group, waitid, Pid, Signal, WaitId, WaitIdOptions};
use std::{
    io::{self, Read},
    os::unix::process::{CommandExt, ExitStatusExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

pub struct NativeGitCommands {
    executable: PathBuf,
    cancelled: Arc<AtomicBool>,
}

impl NativeGitCommands {
    #[must_use]
    pub fn new(cancelled: Arc<AtomicBool>) -> Self {
        Self {
            executable: "git".into(),
            cancelled,
        }
    }

    #[must_use]
    pub fn with_executable(executable: PathBuf, cancelled: Arc<AtomicBool>) -> Self {
        Self {
            executable,
            cancelled,
        }
    }
}

struct OwnedChild {
    child: Child,
    reaped: bool,
}

impl OwnedChild {
    fn kill(&self) -> io::Result<()> {
        match kill_process_group(Pid::from_child(&self.child), Signal::KILL) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
    fn finished(&self) -> io::Result<bool> {
        match waitid(
            WaitId::Pid(Pid::from_child(&self.child)),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        ) {
            Ok(value) => Ok(value.is_some()),
            Err(rustix::io::Errno::INTR) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
    fn finish(&mut self) -> io::Result<i32> {
        self.kill()?;
        let status = self.child.wait()?;
        self.reaped = true;
        Ok(status
            .code()
            .unwrap_or_else(|| -status.signal().unwrap_or(1)))
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.reaped {
            if let Err(error) = self.kill() {
                eprintln!("cluster git process cleanup failed: {error}");
            }
            if let Err(error) = self.child.wait() {
                eprintln!("cluster git process reap failed: {error}");
            }
        }
    }
}

fn nonblocking(pipe: &impl std::os::fd::AsFd) -> io::Result<()> {
    let flags = rustix::fs::fcntl_getfl(pipe)?;
    Ok(rustix::fs::fcntl_setfl(
        pipe,
        flags | rustix::fs::OFlags::NONBLOCK,
    )?)
}

fn drain(pipe: &mut impl Read, output: &mut Vec<u8>) -> io::Result<bool> {
    let mut buffer = [0; 8192];
    for _ in 0..8 {
        match pipe.read(&mut buffer) {
            Ok(0) => return Ok(true),
            Ok(length) => output.extend_from_slice(&buffer[..length]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
            Err(error) => return Err(error),
        }
    }
    Ok(false)
}

impl NativeGitCommands {
    fn execute(&self, repo: &Path, args: &[&str], timeout: Duration) -> io::Result<GitOutput> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "git lookup cancelled",
            ));
        }
        let started = Instant::now();
        let child = Command::new(&self.executable)
            .process_group(0)
            .arg("-C")
            .arg(repo)
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut owned = OwnedChild {
            child,
            reaped: false,
        };
        let mut stdout = owned
            .child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("git stdout unavailable"))?;
        let mut stderr = owned
            .child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("git stderr unavailable"))?;
        nonblocking(&stdout)?;
        nonblocking(&stderr)?;
        let mut output = Vec::new();
        let mut errors = Vec::new();
        let mut code = None;
        loop {
            if self.cancelled.load(Ordering::Acquire) {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "git lookup cancelled",
                ));
            }
            if started.elapsed() >= timeout {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "git lookup timed out",
                ));
            }
            let out_done = drain(&mut stdout, &mut output)?;
            let err_done = drain(&mut stderr, &mut errors)?;
            errors.clear();
            if code.is_none() && owned.finished()? {
                code = Some(owned.finish()?);
            }
            if let Some(code) = code.filter(|_| out_done && err_done) {
                let stdout = String::from_utf8_lossy(&output)
                    .replace("\r\n", "\n")
                    .replace('\r', "\n");
                return Ok(GitOutput { code, stdout });
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}

impl GitCommands for NativeGitCommands {
    fn run(&mut self, repo: &Path, args: &[&str], timeout: Duration) -> GitOutput {
        self.execute(repo, args, timeout)
            .unwrap_or_else(|_| GitOutput {
                code: 124,
                stdout: String::new(),
            })
    }
}
