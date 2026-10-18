//! Interactive probe ownership, retaining a waitable leader through group cleanup.
use crate::seal::Result;
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
    process::{kill_process_group, waitid, Pid, Signal, WaitId, WaitIdOptions},
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::File,
    io::{self, Read, Write},
    os::{fd::AsFd, unix::process::CommandExt},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub(crate) struct Cancellation {
    flag: Arc<AtomicBool>,
    signal: Arc<AtomicUsize>,
    registrations: Vec<signal_hook::SigId>,
}
impl Cancellation {
    pub(crate) fn new() -> Result<Self> {
        let mut value = Self {
            flag: Arc::new(AtomicBool::new(false)),
            signal: Arc::new(AtomicUsize::new(0)),
            registrations: Vec::new(),
        };
        for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            value.registrations.push(signal_hook::flag::register_usize(
                signal,
                Arc::clone(&value.signal),
                signal as usize,
            )?);
            value.registrations.push(signal_hook::flag::register(
                signal,
                Arc::clone(&value.flag),
            )?);
        }
        Ok(value)
    }
    pub(crate) fn check(&self) -> Result<()> {
        if self.flag.load(Ordering::Relaxed) {
            Err("Version reference interrupted".into())
        } else {
            Ok(())
        }
    }
    pub(crate) fn flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.flag)
    }
    pub(crate) fn signal(&self) -> Option<i32> {
        i32::try_from(self.signal.load(Ordering::Relaxed))
            .ok()
            .filter(|v| *v != 0)
    }
}
impl Drop for Cancellation {
    fn drop(&mut self) {
        for registration in self.registrations.drain(..) {
            signal_hook::low_level::unregister(registration);
        }
    }
}
pub(crate) struct Probe {
    child: Child,
    pid: Pid,
    reaped: bool,
    buffer: Vec<u8>,
    eof: bool,
}
fn nonblocking(fd: impl AsFd) -> Result<()> {
    let flags = fcntl_getfl(&fd)?;
    fcntl_setfl(fd, flags | OFlags::NONBLOCK)?;
    Ok(())
}
impl Probe {
    pub(crate) fn spawn(
        argv: &[OsString],
        cwd: &Path,
        env: &BTreeMap<OsString, OsString>,
        stderr: File,
    ) -> Result<Self> {
        let first = argv.first().ok_or("empty Version probe command")?;
        let child = Command::new(first)
            .args(&argv[1..])
            .current_dir(cwd)
            .env_clear()
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr)
            .process_group(0)
            .spawn()?;
        let owner = Self {
            pid: Pid::from_child(&child),
            child,
            reaped: false,
            buffer: Vec::new(),
            eof: false,
        };
        nonblocking(owner.child.stdin.as_ref().ok_or("probe stdin absent")?)?;
        nonblocking(owner.child.stdout.as_ref().ok_or("probe stdout absent")?)?;
        Ok(owner)
    }
    fn status(&self) -> rustix::io::Result<Option<i32>> {
        loop {
            match waitid(
                WaitId::Pid(self.pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            ) {
                Ok(status) => {
                    return Ok(status.map(|s| {
                        s.exit_status()
                            .unwrap_or_else(|| -s.terminating_signal().unwrap_or(1))
                    }))
                }
                Err(rustix::io::Errno::INTR) => continue,
                Err(error) => return Err(error),
            }
        }
    }
    pub(crate) fn write(&mut self, bytes: &[u8], cancel: &Cancellation) -> Result<()> {
        self.write_until(bytes, cancel, Instant::now() + Duration::from_secs(10))
    }
    pub(crate) fn write_until(
        &mut self,
        bytes: &[u8],
        cancel: &Cancellation,
        end: Instant,
    ) -> Result<()> {
        let mut at = 0;
        while at < bytes.len() {
            cancel.check()?;
            if Instant::now() >= end {
                return Err("Version probe write timeout".into());
            }
            let pipe = self
                .child
                .stdin
                .as_mut()
                .ok_or("probe input already closed")?;
            match pipe.write(&bytes[at..]) {
                Ok(0) => return Err("Version probe closed stdin".into()),
                Ok(n) => at += n,
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    let mut fds = [PollFd::new(pipe, PollFlags::OUT)];
                    poll(
                        &mut fds,
                        Some(&Timespec::try_from(Duration::from_millis(10))?),
                    )?;
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
    pub(crate) fn line(&mut self, cancel: &Cancellation) -> Result<Vec<u8>> {
        self.line_until(cancel, Instant::now() + Duration::from_secs(10))
    }
    pub(crate) fn line_until(&mut self, cancel: &Cancellation, end: Instant) -> Result<Vec<u8>> {
        loop {
            cancel.check()?;
            for (i, &b) in self.buffer.iter().enumerate() {
                if b == b'\n' {
                    return Ok(self.buffer.drain(..=i).collect());
                }
                if b == b'\r' && (self.buffer.len() > i + 1 || self.eof) {
                    let n = if self.buffer.get(i + 1) == Some(&b'\n') {
                        i + 2
                    } else {
                        i + 1
                    };
                    return Ok(self.buffer.drain(..n).collect());
                }
            }
            if self.eof {
                let rest = std::mem::take(&mut self.buffer);
                if rest.is_empty() {
                    return Err("Version probe exited before a response".into());
                }
                return Ok(rest);
            }
            if Instant::now() >= end {
                return Err("Version probe response timeout".into());
            }
            let pipe = self.child.stdout.as_mut().ok_or("probe stdout absent")?;
            let mut fds = [PollFd::new(&*pipe, PollFlags::IN)];
            match poll(
                &mut fds,
                Some(&Timespec::try_from(Duration::from_millis(10))?),
            ) {
                Ok(_) => {}
                Err(rustix::io::Errno::INTR) => continue,
                Err(e) => return Err(e.into()),
            }
            let mut bytes = [0; 8192];
            match pipe.read(&mut bytes) {
                Ok(0) => self.eof = true,
                Ok(n) => self.buffer.extend_from_slice(&bytes[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
    pub(crate) fn finish(&mut self, cancel: &Cancellation) -> Result<()> {
        self.child.stdin.take();
        let end = Instant::now() + Duration::from_secs(10);
        loop {
            cancel.check()?;
            if let Some(status) = self.status()? {
                self.close()?;
                return if status == 0 {
                    Ok(())
                } else {
                    Err(format!("Version probe exited {status}").into())
                };
            }
            if Instant::now() >= end {
                return Err("Version probe EOF exit timeout".into());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn close(&mut self) -> Result<()> {
        if self.reaped {
            return Ok(());
        }
        self.child.stdin.take();
        // An embedding caller must not steal this child with a global reaper.
        // If ownership was nevertheless lost, never signal its recycled number.
        match self.status() {
            Ok(_) => {}
            Err(rustix::io::Errno::CHILD) => {
                self.reaped = true;
                return Err("Version probe ownership was lost before cleanup".into());
            }
            Err(error) => return Err(error.into()),
        }
        // WNOWAIT has not consumed the child identity. Even an exited shell's
        // descendants are signalled before the leader can be recycled.
        match kill_process_group(self.pid, Signal::KILL) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => {}
            Err(e) => return Err(e.into()),
        }
        self.child.wait()?;
        self.reaped = true;
        Ok(())
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
