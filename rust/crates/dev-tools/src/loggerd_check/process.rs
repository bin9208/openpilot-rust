//! Direct protocol process ownership is armed before any fallible pipe access.
use super::{require, Result};
use std::{
    fs::{self, File},
    io::{Read, Write},
    os::unix::process::ExitStatusExt,
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::{Duration, Instant},
};
struct Starting(Option<Child>);
impl Drop for Starting {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
pub(super) struct Process {
    child: Option<Child>,
    input: Option<ChildStdin>,
    output: ChildStdout,
    pending: Vec<u8>,
    timeout: Duration,
    trace: File,
}
impl Drop for Process {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
impl Process {
    pub fn start(command: &mut Command, output: &Path, timeout: Duration) -> Result<Self> {
        fs::create_dir_all(output)?;
        let error = File::create(output.join("stderr.log"))?;
        let trace = File::create(output.join("stdout.log"))?;
        let child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(error)
            .spawn()?;
        let mut starting = Starting(Some(child));
        let child = starting.0.as_mut().ok_or("protocol child owner absent")?;
        let input = child.stdin.take().ok_or("protocol stdin absent")?;
        let output = child.stdout.take().ok_or("protocol stdout absent")?;
        let owned = Self {
            child: starting.0.take(),
            input: Some(input),
            output,
            pending: Vec::new(),
            timeout,
            trace,
        };
        rustix::fs::fcntl_setfl(
            &owned.output,
            rustix::fs::fcntl_getfl(&owned.output)? | rustix::fs::OFlags::NONBLOCK,
        )?;
        Ok(owned)
    }
    pub fn pid(&self) -> Result<rustix::process::Pid> {
        Ok(rustix::process::Pid::from_child(
            self.child.as_ref().ok_or("protocol child reaped")?,
        ))
    }
    pub fn alive(&mut self) -> Result<()> {
        let child = self.child.as_mut().ok_or("protocol child reaped")?;
        if child.try_wait()?.is_some() {
            self.child = None;
            return Err("protocol child exited early".into());
        }
        Ok(())
    }
    pub fn write(&mut self, bytes: &[u8]) -> Result<()> {
        let input = self.input.as_mut().ok_or("protocol input closed")?;
        input.write_all(bytes)?;
        input.flush()?;
        Ok(())
    }
    pub fn line(&mut self) -> Result<Vec<u8>> {
        let deadline = Instant::now() + self.timeout;
        loop {
            if let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
                let mut line = self.pending.drain(..=end).collect::<Vec<_>>();
                line.pop();
                return Ok(line);
            }
            require(
                Instant::now() < deadline,
                "source encoder protocol deadline exceeded",
            )?;
            let mut bytes = [0; 8192];
            match self.output.read(&mut bytes) {
                Ok(0) => return Err("source encoder protocol closed before acknowledgement".into()),
                Ok(n) => {
                    require(
                        self.pending.len() + n <= 1 << 20,
                        "source encoder stdout exceeded bound",
                    )?;
                    self.trace.write_all(&bytes[..n])?;
                    self.pending.extend_from_slice(&bytes[..n]);
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    std::thread::sleep(Duration::from_millis(1))
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
    pub fn wait(&mut self, timeout: Duration) -> Result<i32> {
        self.input.take();
        let deadline = Instant::now() + timeout;
        loop {
            let child = self
                .child
                .as_mut()
                .ok_or("source encoder was already reaped")?;
            if let Some(status) = child.try_wait()? {
                self.child = None;
                return Ok(status
                    .code()
                    .unwrap_or_else(|| -status.signal().unwrap_or(1)));
            }
            require(
                Instant::now() < deadline,
                "source encoder shutdown deadline exceeded",
            )?;
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
