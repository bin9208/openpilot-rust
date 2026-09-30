//! The external OS reader is owned until terminate/wait completes on every exit path.
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
    process::{kill_process, Pid, Signal},
};
use std::{
    io::{self, Read},
    process::{Child, ChildStdout, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
};

pub struct Journal {
    process: Option<Child>,
    stdout: ChildStdout,
    pending: Vec<u8>,
    eof: bool,
}

impl Journal {
    pub fn spawn() -> io::Result<Self> {
        let mut process = Command::new("journalctl")
            .args(["-f", "-o", "json"])
            .stdout(Stdio::piped())
            .spawn()?;
        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("journalctl stdout was not piped"))?;
        let mut journal = Self {
            process: Some(process),
            stdout,
            pending: Vec::new(),
            eof: false,
        };
        if let Err(error) = fcntl_getfl(&journal.stdout)
            .and_then(|flags| fcntl_setfl(&journal.stdout, flags | OFlags::NONBLOCK))
        {
            journal.terminate()?;
            return Err(error.into());
        }
        Ok(journal)
    }

    pub fn next_line(&mut self, stop: &AtomicBool) -> io::Result<Option<String>> {
        loop {
            if stop.load(Ordering::Relaxed) {
                return Ok(None);
            }
            if let Err(error) = std::str::from_utf8(&self.pending) {
                if error.error_len().is_some() || self.eof {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, error));
                }
            }
            if let Some(position) = self
                .pending
                .iter()
                .position(|byte| matches!(byte, b'\r' | b'\n'))
            {
                let carriage = self.pending[position] == b'\r';
                if !carriage || position + 1 < self.pending.len() || self.eof {
                    let length = position
                        + 1
                        + usize::from(carriage && self.pending.get(position + 1) == Some(&b'\n'));
                    let line: Vec<_> = self.pending.drain(..length).collect();
                    return String::from_utf8(line)
                        .map(Some)
                        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error));
                }
            }
            if self.eof {
                if self.pending.is_empty() {
                    return Ok(None);
                }
                let line = std::mem::take(&mut self.pending);
                return String::from_utf8(line)
                    .map(Some)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error));
            }
            let mut descriptors = [PollFd::new(&self.stdout, PollFlags::IN)];
            match poll(
                &mut descriptors,
                Some(&Timespec {
                    tv_sec: 0,
                    tv_nsec: 100_000_000,
                }),
            ) {
                Ok(0) | Err(rustix::io::Errno::INTR) => continue,
                Ok(_) => {}
                Err(error) => return Err(error.into()),
            }
            let mut bytes = [0_u8; 8192];
            match self.stdout.read(&mut bytes) {
                Ok(0) => self.eof = true,
                Ok(count) => self.pending.extend_from_slice(&bytes[..count]),
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => return Err(error),
            }
        }
    }

    pub fn terminate(&mut self) -> io::Result<()> {
        if let Some(process) = self.process.as_mut() {
            if process.try_wait()?.is_none() {
                let pid = i32::try_from(process.id())
                    .ok()
                    .and_then(Pid::from_raw)
                    .ok_or_else(|| io::Error::other("invalid journalctl child PID"))?;
                match kill_process(pid, Signal::TERM) {
                    Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                    Err(error) => return Err(error.into()),
                }
            }
            process.wait()?;
            self.process = None;
        }
        Ok(())
    }
}

impl Drop for Journal {
    fn drop(&mut self) {
        if let Err(error) = self.terminate() {
            eprintln!("journald child cleanup: {error}");
        }
    }
}
