//! Real child/pipe owner. Exited leaders remain waitable until session cleanup,
//! so no later signal can target a reused numeric process-group ID.
use super::{Runtime, Spawn};
use crate::{qa_process::Captured, seal::Result};
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
    process::{kill_process, kill_process_group, waitid, Pid, Signal, WaitId, WaitIdOptions},
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::{self, Read, Write},
    os::fd::AsFd,
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
#[derive(Debug)]
pub(super) struct Interrupted;
impl std::fmt::Display for Interrupted {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        output.write_str("lagd checker interrupted")
    }
}
impl std::error::Error for Interrupted {}
struct ChildOwner {
    child: Child,
    pid: Pid,
    session: bool,
    reaped: bool,
    buffer: Vec<u8>,
    stdout_eof: bool,
}
impl ChildOwner {
    fn poll(&self) -> Result<Option<i32>> {
        loop {
            match waitid(
                WaitId::Pid(self.pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            ) {
                Ok(value) => {
                    return Ok(value.map(|v| {
                        v.exit_status()
                            .unwrap_or_else(|| -v.terminating_signal().unwrap_or(1))
                    }))
                }
                Err(rustix::io::Errno::INTR) => continue,
                Err(error) => return Err(error.into()),
            }
        }
    }
    fn close(&mut self) -> Result<()> {
        if self.reaped {
            return Ok(());
        }
        self.child.stdin.take();
        // WNOWAIT leaves the leader's exact PID reserved through this group signal,
        // even if the shell/daemon has exited while its descendant still owns pipes.
        if self.session {
            match kill_process_group(self.pid, Signal::KILL) {
                Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                Err(error) => return Err(error.into()),
            }
        } else if self.poll()?.is_none() {
            match kill_process(self.pid, Signal::KILL) {
                Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                Err(error) => return Err(error.into()),
            }
        }
        let end = Instant::now() + Duration::from_secs(3);
        while self.poll()?.is_none() {
            if Instant::now() >= end {
                return Err("lagd child cleanup wait exceeded original three seconds".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        self.child.wait()?;
        self.reaped = true;
        Ok(())
    }
}
impl Drop for ChildOwner {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.close();
        }
    }
}
pub struct NativeRuntime {
    children: BTreeMap<usize, ChildOwner>,
    next: usize,
    interrupted: Arc<AtomicBool>,
    registration: signal_hook::SigId,
}
impl NativeRuntime {
    pub fn new() -> Result<Self> {
        let interrupted = Arc::new(AtomicBool::new(false));
        let registration =
            signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&interrupted))?;
        Ok(Self {
            children: BTreeMap::new(),
            next: 0,
            interrupted,
            registration,
        })
    }
    fn check(&self) -> Result<()> {
        if self.interrupted.load(Ordering::Relaxed) {
            Err(Interrupted.into())
        } else {
            Ok(())
        }
    }
    fn child(&mut self, id: usize) -> Result<&mut ChildOwner> {
        self.children
            .get_mut(&id)
            .ok_or_else(|| "lagd child already closed".into())
    }
}
impl Drop for NativeRuntime {
    fn drop(&mut self) {
        self.children.clear();
        signal_hook::low_level::unregister(self.registration);
    }
}
fn nonblocking(fd: impl AsFd) -> Result<()> {
    let flags = fcntl_getfl(&fd)?;
    fcntl_setfl(fd, flags | OFlags::NONBLOCK)?;
    Ok(())
}
fn file(path: &Path) -> Result<fs::File> {
    Ok(fs::OpenOptions::new().write(true).open(path)?)
}
fn line_end(bytes: &[u8], eof: bool) -> Option<usize> {
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            return Some(index + 1);
        }
        if *byte == b'\r' {
            return if bytes.get(index + 1) == Some(&b'\n') {
                Some(index + 2)
            } else if index + 1 < bytes.len() || eof {
                Some(index + 1)
            } else {
                None
            };
        }
    }
    None
}
fn read_once(pipe: &mut impl Read, out: &mut Vec<u8>) -> Result<bool> {
    let mut buffer = [0; 8192];
    match pipe.read(&mut buffer) {
        Ok(0) => Ok(true),
        Ok(count) => {
            out.extend_from_slice(&buffer[..count]);
            Ok(false)
        }
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) =>
        {
            Ok(false)
        }
        Err(error) => Err(error.into()),
    }
}
impl Runtime for NativeRuntime {
    fn environment(&self) -> BTreeMap<OsString, OsString> {
        std::env::vars_os().collect()
    }
    fn spawn(&mut self, spec: &Spawn) -> Result<usize> {
        self.check()?;
        let program = spec.argv.first().ok_or("empty lagd child command")?;
        let mut command = Command::new(program);
        command
            .args(&spec.argv[1..])
            .env_clear()
            .envs(&spec.env)
            .stdin(if spec.stdin_pipe {
                Stdio::piped()
            } else {
                Stdio::inherit()
            });
        let output =
            |handle: &Option<Arc<fs::File>>, path: &Option<std::path::PathBuf>| -> Result<Stdio> {
                if let Some(handle) = handle {
                    return Ok(handle.try_clone()?.into());
                }
                Ok(match path {
                    Some(path) => file(path)?.into(),
                    None => Stdio::piped(),
                })
            };
        if spec.stdout_file.is_none()
            && spec.stderr_file.is_none()
            && spec.stdout.is_some()
            && spec.stdout == spec.stderr
        {
            let file = file(spec.stdout.as_deref().unwrap())?;
            command.stdout(file.try_clone()?).stderr(file);
        } else {
            command
                .stdout(output(&spec.stdout_file, &spec.stdout)?)
                .stderr(output(&spec.stderr_file, &spec.stderr)?);
        }
        if spec.new_session {
            openpilot_process_limits::new_session(&mut command);
        }
        let child = command.spawn()?;
        let pid = Pid::from_child(&child);
        let owner = ChildOwner {
            child,
            pid,
            session: spec.new_session,
            reaped: false,
            buffer: Vec::new(),
            stdout_eof: false,
        };
        // Owner exists before any fallible post-spawn descriptor setup.
        if let Some(pipe) = &owner.child.stdout {
            nonblocking(pipe)?;
        }
        if let Some(pipe) = &owner.child.stderr {
            nonblocking(pipe)?;
        }
        if let Some(pipe) = &owner.child.stdin {
            nonblocking(pipe)?;
        }
        let id = self.next;
        self.next = self.next.checked_add(1).ok_or("too many lagd children")?;
        self.children.insert(id, owner);
        Ok(id)
    }
    fn read_line(&mut self, id: usize, timeout: Option<Duration>) -> Result<Vec<u8>> {
        let end = timeout.map(|v| Instant::now() + v);
        loop {
            self.check()?;
            let child = self.child(id)?;
            if let Some(end) = line_end(&child.buffer, child.stdout_eof) {
                return Ok(child.buffer.drain(..end).collect());
            }
            if child.stdout_eof {
                return Ok(std::mem::take(&mut child.buffer));
            }
            if end.is_some_and(|end| Instant::now() >= end) {
                return Err("lagd peer response timeout".into());
            }
            let pipe = child
                .child
                .stdout
                .as_ref()
                .ok_or("lagd child stdout is not a pipe")?;
            let mut descriptors = [PollFd::new(pipe, PollFlags::IN)];
            let remaining = end.map_or(Duration::from_millis(20), |end| {
                end.saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(20))
            });
            let timeout = Timespec::try_from(remaining)?;
            match poll(&mut descriptors, Some(&timeout)) {
                Ok(_) => {}
                Err(rustix::io::Errno::INTR) => continue,
                Err(error) => return Err(error.into()),
            }
            let pipe = child
                .child
                .stdout
                .as_mut()
                .ok_or("lagd stdout disappeared")?;
            child.stdout_eof = read_once(pipe, &mut child.buffer)?;
        }
    }
    fn write_flush(&mut self, id: usize, bytes: &[u8]) -> Result<()> {
        let mut at = 0;
        while at < bytes.len() {
            self.check()?;
            let child = self.child(id)?;
            let pipe = child
                .child
                .stdin
                .as_mut()
                .ok_or("lagd child stdin is not a pipe")?;
            match pipe.write(&bytes[at..]) {
                Ok(0) => return Err("lagd peer stdin closed".into()),
                Ok(count) => at += count,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    let mut fds = [PollFd::new(&*pipe, PollFlags::OUT)];
                    let timeout = Timespec::try_from(Duration::from_millis(20))?;
                    match poll(&mut fds, Some(&timeout)) {
                        Ok(_) | Err(rustix::io::Errno::INTR) => {}
                        Err(error) => return Err(error.into()),
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
    fn poll(&mut self, id: usize) -> Result<Option<i32>> {
        self.check()?;
        self.child(id)?.poll()
    }
    fn signal(&mut self, id: usize, signal: i32) -> Result<()> {
        self.check()?;
        let child = self.child(id)?;
        if child.poll()?.is_some() {
            return Ok(());
        }
        let signal = match signal {
            2 => Signal::INT,
            9 => Signal::KILL,
            15 => Signal::TERM,
            _ => return Err("unsupported lagd QA signal".into()),
        };
        match kill_process(child.pid, signal) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
    fn wait(&mut self, id: usize, timeout: Duration) -> Result<i32> {
        let end = Instant::now() + timeout;
        loop {
            self.check()?;
            if let Some(code) = self.child(id)?.poll()? {
                return Ok(code);
            }
            if Instant::now() >= end {
                return Err("lagd child wait timeout".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn communicate(&mut self, id: usize, timeout: Duration) -> Result<Captured> {
        self.check()?;
        let child = self.child(id)?;
        child.child.stdin.take();
        let mut stdout = std::mem::take(&mut child.buffer);
        let mut stderr = Vec::new();
        let mut out_eof = child.stdout_eof;
        let mut err_eof = child.child.stderr.is_none();
        let end = Instant::now() + timeout;
        loop {
            self.check()?;
            let child = self.child(id)?;
            if !out_eof {
                out_eof = read_once(
                    child
                        .child
                        .stdout
                        .as_mut()
                        .ok_or("lagd stdout is not a pipe")?,
                    &mut stdout,
                )?;
            }
            if !err_eof {
                err_eof = read_once(
                    child
                        .child
                        .stderr
                        .as_mut()
                        .ok_or("lagd stderr is not a pipe")?,
                    &mut stderr,
                )?;
            }
            let code = child.poll()?;
            if let Some(code) = code {
                if out_eof && err_eof {
                    return Ok(Captured {
                        code,
                        stdout,
                        stderr,
                    });
                }
            }
            if Instant::now() >= end {
                return Err("lagd communicate timeout".into());
            }
            let mut descriptors = Vec::new();
            if !out_eof {
                descriptors.push(PollFd::new(
                    child.child.stdout.as_ref().unwrap(),
                    PollFlags::IN,
                ));
            }
            if !err_eof {
                descriptors.push(PollFd::new(
                    child.child.stderr.as_ref().unwrap(),
                    PollFlags::IN,
                ));
            }
            let timeout = Timespec::try_from(
                end.saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(20)),
            )?;
            match poll(&mut descriptors, Some(&timeout)) {
                Ok(_) | Err(rustix::io::Errno::INTR) => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    fn executable_is(&mut self, id: usize, path: &Path) -> Result<bool> {
        self.check()?;
        let pid = self.child(id)?.pid;
        Ok(crate::path_resolve::resolve_non_strict(Path::new(&format!(
            "/proc/{}/exe",
            pid.as_raw_nonzero().get()
        )))? == crate::path_resolve::resolve_non_strict(path)?)
    }
    fn close(&mut self, id: usize) -> Result<()> {
        self.child(id)?.close()?;
        self.children.remove(&id);
        Ok(())
    }
    fn sleep(&mut self, time: Duration) -> Result<()> {
        let end = Instant::now() + time;
        loop {
            self.check()?;
            let remaining = end.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(());
            }
            std::thread::sleep(remaining.min(Duration::from_millis(2)));
        }
    }
    fn monotonic(&mut self) -> Result<f64> {
        let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        Ok(time.tv_sec as f64 + time.tv_nsec as f64 * 1e-9)
    }
}
