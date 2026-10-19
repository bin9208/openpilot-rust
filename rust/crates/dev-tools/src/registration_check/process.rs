//! Interactive child ownership inside one outer qa_process-owned process group.
//! Direct leaders remain unreaped until all signaling/cleanup decisions finish.
use crate::seal::Result;
use rustix::process::{waitid, Pid, Signal, WaitId, WaitIdOptions};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

pub(super) struct Interactive {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    stdout: ChildStdout,
    buffer: Vec<u8>,
}
impl Interactive {
    pub(super) fn spawn(
        argv: &[OsString],
        env: &BTreeMap<OsString, OsString>,
        stderr: &Path,
    ) -> Result<Self> {
        let (program, args) = argv
            .split_first()
            .ok_or("empty registration child command")?;
        let mut child = Command::new(program)
            .args(args)
            .env_clear()
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(File::create(stderr)?))
            .spawn()?;
        let stdin = child.stdin.take().ok_or("registration stdin absent")?;
        let stdout = child.stdout.take().ok_or("registration stdout absent")?;
        let flags = rustix::fs::fcntl_getfl(&stdout)?;
        rustix::fs::fcntl_setfl(&stdout, flags | rustix::fs::OFlags::NONBLOCK)?;
        Ok(Self {
            child: Some(child),
            stdin: Some(stdin),
            stdout,
            buffer: Vec::new(),
        })
    }
    pub(super) fn id(&self) -> Result<Pid> {
        Pid::from_raw(
            self.child
                .as_ref()
                .ok_or("registration child already reaped")?
                .id() as i32,
        )
        .ok_or_else(|| "invalid registration PID".into())
    }
    pub(super) fn exited(&self) -> Result<bool> {
        Ok(waitid(
            WaitId::Pid(self.id()?),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        )?
        .is_some())
    }
    pub(super) fn send(&mut self, bytes: &[u8]) -> Result<()> {
        let input = self
            .stdin
            .as_mut()
            .ok_or("registration child input closed")?;
        input.write_all(bytes)?;
        input.flush()?;
        Ok(())
    }
    pub(super) fn line(&mut self, timeout: Duration) -> Result<Vec<u8>> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(end) = self.buffer.iter().position(|b| *b == b'\n') {
                let line = self.buffer.drain(..=end).collect::<Vec<_>>();
                return Ok(line[..line.len() - 1].to_vec());
            }
            let mut bytes = [0; 65536];
            match self.stdout.read(&mut bytes) {
                Ok(0) => return Err("registration child ended before JSON result".into()),
                Ok(size) => self.buffer.extend_from_slice(&bytes[..size]),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(e.into()),
            }
            if Instant::now() >= deadline {
                return Err("registration did not finish within 120s".into());
            }
            if self.exited()? && self.buffer.is_empty() {
                return Err("registration child exited before result".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    pub(super) fn signal(&self, signal: Signal) -> Result<bool> {
        if self.exited()? {
            return Ok(false);
        }
        match rustix::process::kill_process(self.id()?, signal) {
            Ok(()) => Ok(true),
            Err(rustix::io::Errno::SRCH) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
    pub(super) fn finish(&mut self, timeout: Duration) -> Result<ExitStatus> {
        self.stdin.take();
        let deadline = Instant::now() + timeout;
        while !self.exited()? {
            if Instant::now() >= deadline {
                return Err(
                    "registration child did not exit within original cleanup deadline".into(),
                );
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        Ok(self
            .child
            .take()
            .ok_or("registration child reaped twice")?
            .wait()?)
    }
}
impl Drop for Interactive {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // WNOWAIT retains the PID; kill is never sent after wait releases its identity.
            let pid = Pid::from_raw(child.id() as i32);
            let exited = pid
                .and_then(|pid| {
                    waitid(
                        WaitId::Pid(pid),
                        WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
                    )
                    .ok()
                })
                .flatten()
                .is_some();
            if !exited {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}
pub(super) fn append(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(bytes)?;
    Ok(())
}
