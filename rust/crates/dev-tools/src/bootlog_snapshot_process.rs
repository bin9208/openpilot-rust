//! Owned real-process stream adapter. Deadlines retain the original15/5second
//! protocol bounds; an unreaped leader reserves the process-group identity.
use crate::seal::Result;
use rustix::{
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
    process::{kill_process_group, waitid, Pid, Signal, WaitId, WaitIdOptions},
};
use std::{
    fs::File,
    io::{Read, Write},
    os::{fd::AsRawFd, unix::process::CommandExt},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::{Duration, Instant},
};
pub struct Process {
    child: Child,
    input: Option<ChildStdin>,
    output: ChildStdout,
    pending: Vec<u8>,
    reaped: bool,
}
impl Process {
    pub fn start(mut command: Command, stderr: File, marker: &File) -> Result<Self> {
        use command_fds::{CommandFdExt, FdMapping};
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr)
            .process_group(0);
        command.fd_mappings(vec![FdMapping {
            parent_fd: marker.try_clone()?.into(),
            child_fd: marker.as_raw_fd(),
        }])?;
        let child = command.spawn()?;
        drop(command);
        Self::own(child)
    }
    fn own(mut child: Child) -> Result<Self> {
        let (Some(input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err("missing snapshot protocol streams".into());
        };
        let owner = Self {
            child,
            input: Some(input),
            output,
            pending: vec![],
            reaped: false,
        };
        fcntl_setfl(
            &owner.output,
            fcntl_getfl(&owner.output)? | OFlags::NONBLOCK,
        )?;
        if let Some(input) = &owner.input {
            fcntl_setfl(input, fcntl_getfl(input)? | OFlags::NONBLOCK)?;
        }
        Ok(owner)
    }
    pub fn from_command(mut command: Command) -> Result<Self> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .process_group(0);
        Self::own(command.spawn()?)
    }
    pub fn line(&mut self) -> Result<serde_json::Value> {
        let end = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(i) = self.pending.iter().position(|b| *b == b'\n') {
                let bytes = self.pending.drain(..=i).collect::<Vec<_>>();
                return Ok(serde_json::from_str(std::str::from_utf8(&bytes)?)?);
            }
            let mut buf = [0; 4096];
            match self.output.read(&mut buf) {
                Ok(0) => {
                    if self.pending.is_empty() {
                        return Err("snapshot phase stream ended".into());
                    }
                    return Ok(serde_json::from_str(std::str::from_utf8(
                        &std::mem::take(&mut self.pending),
                    )?)?);
                }
                Ok(n) => self.pending.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.into()),
            }
            if Instant::now() >= end {
                return Err("snapshot phase acknowledgement".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    pub fn finish_input(&mut self) -> Result<()> {
        let end = Instant::now() + Duration::from_secs(5);
        if let Some(mut input) = self.input.take() {
            let mut bytes = b"finish\n".as_slice();
            while !bytes.is_empty() {
                match input.write(bytes) {
                    Ok(0) => return Err("snapshot input write failed".into()),
                    Ok(n) => bytes = &bytes[n..],
                    Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => break,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        if Instant::now() >= end {
                            return Err("snapshot input deadline".into());
                        }
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Err(e) => return Err(e.into()),
                }
            }
        }
        Ok(())
    }
    pub fn running(&mut self) -> Result<bool> {
        if self.reaped {
            return Ok(false);
        }
        // Observe without reaping. The leader reserves its process-group ID
        // until Drop terminates remaining descendants and reaps it.
        let pid = Pid::from_raw(self.child.id() as i32).ok_or("invalid snapshot PID")?;
        match waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        )? {
            Some(status) => {
                if status.exit_status() != Some(0) {
                    return Err("snapshot process exited unsuccessfully".into());
                }
                Ok(false)
            }
            None => Ok(true),
        }
    }

    pub fn wait(&mut self, seconds: u64) -> Result<()> {
        let end = Instant::now() + Duration::from_secs(seconds);
        while self.running()? {
            if Instant::now() >= end {
                return Err("snapshot process wait deadline".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        Ok(())
    }
    pub fn drain(&mut self) -> Result<()> {
        let end = Instant::now() + Duration::from_secs(5);
        let mut bytes = [0; 8192];
        loop {
            match self.output.read(&mut bytes) {
                Ok(0) => return Ok(()),
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= end {
                        return Err("snapshot descendant output deadline".into());
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if !self.reaped {
            if let Some(pid) = Pid::from_raw(self.child.id() as i32) {
                let _ = kill_process_group(pid, Signal::KILL);
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
            self.reaped = true;
        }
    }
}
