use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
};
use std::{
    fs::File,
    io::{self, Read},
    os::fd::OwnedFd,
    process::{Child, ExitStatus, Output},
    time::{Duration, Instant},
};

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("captured command timed out after {timeout:?}")]
    Timeout {
        timeout: Duration,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Launch(#[from] crate::Error),
}
struct Pipe {
    file: File,
    bytes: Vec<u8>,
    eof: bool,
}
impl Pipe {
    fn new(fd: OwnedFd) -> Self {
        Self {
            file: fd.into(),
            bytes: Vec::new(),
            eof: false,
        }
    }
    fn nonblocking(&self) -> io::Result<()> {
        let flags = fcntl_getfl(&self.file)?;
        fcntl_setfl(&self.file, flags | OFlags::NONBLOCK)?;
        Ok(())
    }
    fn read(&mut self) -> io::Result<()> {
        if self.eof {
            return Ok(());
        }
        let mut buffer = [0; 32768];
        match self.file.read(&mut buffer) {
            Ok(0) => self.eof = true,
            Ok(length) => self.bytes.extend_from_slice(&buffer[..length]),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error),
        }
        Ok(())
    }
}
fn capture(
    child: &mut Child,
    stdout: &mut Pipe,
    stderr: &mut Pipe,
    started: Instant,
    timeout: Duration,
) -> Result<ExitStatus, CaptureError> {
    loop {
        let Some(remaining) = timeout
            .checked_sub(started.elapsed())
            .filter(|value| !value.is_zero())
        else {
            return Err(CaptureError::Timeout {
                timeout,
                stdout: std::mem::take(&mut stdout.bytes),
                stderr: std::mem::take(&mut stderr.bytes),
            });
        };
        stdout.read()?;
        stderr.read()?;
        if let Some(status) = child.try_wait()? {
            if stdout.eof && stderr.eof {
                return Ok(status);
            }
        }
        let mut descriptors = Vec::with_capacity(2);
        if !stdout.eof {
            descriptors.push(PollFd::new(&stdout.file, PollFlags::IN));
        }
        if !stderr.eof {
            descriptors.push(PollFd::new(&stderr.file, PollFlags::IN));
        }
        let wait = Timespec::try_from(remaining.min(Duration::from_millis(10)))
            .map_err(io::Error::other)?;
        match poll(&mut descriptors, Some(&wait)) {
            Ok(_) | Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(io::Error::from(error).into()),
        }
    }
}
pub fn capture_output(child: &mut Child, timeout: Duration) -> Result<Output, CaptureError> {
    let started = Instant::now();
    let mut stdout = None;
    let mut stderr = None;
    let result = (|| {
        let pipe = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("captured stdout pipe missing"))?;
        stdout = Some(Pipe::new(pipe.into()));
        let pipe = child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("captured stderr pipe missing"))?;
        stderr = Some(Pipe::new(pipe.into()));
        let stdout = stdout
            .as_mut()
            .ok_or_else(|| io::Error::other("captured stdout initialization failed"))?;
        let stderr = stderr
            .as_mut()
            .ok_or_else(|| io::Error::other("captured stderr initialization failed"))?;
        stdout.nonblocking()?;
        stderr.nonblocking()?;
        let status = capture(child, stdout, stderr, started, timeout)?;
        Ok(Output {
            status,
            stdout: std::mem::take(&mut stdout.bytes),
            stderr: std::mem::take(&mut stderr.bytes),
        })
    })();
    if result.is_err() {
        let killed = child.kill();
        child.wait()?;
        if let Err(error) = killed {
            if !matches!(
                error.kind(),
                io::ErrorKind::InvalidInput | io::ErrorKind::NotFound
            ) {
                return Err(error.into());
            }
        }
        drop(child.stdout.take());
        drop(child.stderr.take());
    }
    result
}

#[cfg(test)]
#[path = "capture_tests.rs"]
mod tests;
