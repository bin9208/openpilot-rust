//! Direct children stay inside the outer NativeRunner-owned worker group.
//! Keep leaders unreaped until explicit finish; never signal a recycled PID.
use super::{require, Result};
use rustix::{
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
    process::{kill_process, waitid, Pid, Signal, WaitId, WaitIdOptions},
};
use serde_json::json;
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    os::unix::process::ExitStatusExt,
    path::Path,
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio},
    time::{Duration, Instant},
};
pub struct Process {
    child: Option<Child>,
    input: Option<ChildStdin>,
    output: ChildStdout,
    error: ChildStderr,
    stdout_eof: bool,
    stderr_eof: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    parsed: usize,
    pub ready: bool,
    pub deletions: Vec<Instant>,
    deadline: Instant,
    out_file: File,
    err_file: File,
}
impl Process {
    pub fn spawn(command: &mut Command, output: &Path) -> Result<Self> {
        fs::create_dir_all(output)?;
        let out_file = File::create(output.join("stdout.bin"))?;
        let err_file = File::create(output.join("stderr.bin"))?;
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        // No fallible work before the owner is armed.
        let input = child.stdin.take();
        let stdout = child.stdout.take().ok_or("child stdout absent");
        let stderr = child.stderr.take().ok_or("child stderr absent");
        let (output, error) = match (stdout, stderr) {
            (Ok(o), Ok(e)) => (o, e),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("child capture pipe absent".into());
            }
        };
        let owned = Self {
            child: Some(child),
            input,
            output,
            error,
            stdout_eof: false,
            stderr_eof: false,
            stdout: Vec::new(),
            stderr: Vec::new(),
            parsed: 0,
            ready: false,
            deletions: Vec::new(),
            deadline: Instant::now() + Duration::from_secs(5),
            out_file,
            err_file,
        };
        fcntl_setfl(
            &owned.output,
            fcntl_getfl(&owned.output)? | OFlags::NONBLOCK,
        )?;
        fcntl_setfl(&owned.error, fcntl_getfl(&owned.error)? | OFlags::NONBLOCK)?;
        Ok(owned)
    }
    fn read(
        pipe: &mut impl Read,
        bytes: &mut Vec<u8>,
        eof: &mut bool,
        file: &mut File,
    ) -> Result<()> {
        loop {
            let mut block = [0; 8192];
            match pipe.read(&mut block) {
                Ok(0) => {
                    *eof = true;
                    break;
                }
                Ok(n) => {
                    require(bytes.len() + n <= 1 << 20, "deleter process output bound")?;
                    file.write_all(&block[..n])?;
                    bytes.extend_from_slice(&block[..n]);
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
    pub fn drain(&mut self) -> Result<()> {
        require(
            Instant::now() < self.deadline,
            "deleter process deadline exceeded",
        )?;
        Self::read(
            &mut self.output,
            &mut self.stdout,
            &mut self.stdout_eof,
            &mut self.out_file,
        )?;
        Self::read(
            &mut self.error,
            &mut self.stderr,
            &mut self.stderr_eof,
            &mut self.err_file,
        )?;
        while let Some(length) = self.stderr[self.parsed..].iter().position(|b| *b == b'\n') {
            let end = self.parsed + length + 1;
            let line = std::str::from_utf8(&self.stderr[self.parsed..end])?;
            if line.contains("deleter: ready") {
                self.ready = true;
            }
            if line.contains("deleting ") {
                self.deletions.push(Instant::now());
            }
            self.parsed = end;
        }
        Ok(())
    }
    pub fn exited(&self) -> Result<bool> {
        let child = self.child.as_ref().ok_or("deleter child already reaped")?;
        Ok(waitid(
            WaitId::Pid(Pid::from_child(child)),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        )?
        .is_some())
    }
    pub fn send(&mut self, bytes: &[u8]) -> Result<()> {
        require(bytes.len() <= 65536, "deleter request bound")?;
        self.input
            .as_mut()
            .ok_or("deleter stdin closed")?
            .write_all(bytes)?;
        self.input.as_mut().ok_or("deleter stdin closed")?.flush()?;
        Ok(())
    }
    pub fn line(&mut self, cursor: &mut usize) -> Result<Vec<u8>> {
        loop {
            self.drain()?;
            if let Some(n) = self.stdout[*cursor..].iter().position(|b| *b == b'\n') {
                let end = *cursor + n + 1;
                let row = self.stdout[*cursor..end].to_vec();
                *cursor = end;
                return Ok(row);
            }
            require(
                !self.exited()? || !self.stdout_eof,
                "deleter trace exited without response",
            )?;
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    pub fn signal(&mut self, signal: Signal) -> Result<()> {
        require(!self.exited()?, "deleter exited before requested signal")?;
        kill_process(
            Pid::from_child(self.child.as_ref().ok_or("deleter child absent")?),
            signal,
        )?;
        Ok(())
    }
    pub fn finish(&mut self) -> Result<i32> {
        self.input = None;
        loop {
            self.drain()?;
            if self.exited()? && self.stdout_eof && self.stderr_eof {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let status = self.child.as_mut().ok_or("deleter child absent")?.wait()?;
        self.child = None;
        Ok(status
            .code()
            .unwrap_or_else(|| -status.signal().unwrap_or(1)))
    }
    pub fn summary(&self, code: i32) -> Result<serde_json::Value> {
        let intervals = self
            .deletions
            .windows(2)
            .map(|p| p[1].duration_since(p[0]).as_secs_f64())
            .collect::<Vec<_>>();
        Ok(
            json!({"exit":code,"ready":self.ready,"stderr":std::str::from_utf8(&self.stderr)?.replace("\r\n","\n").replace('\r',"\n"),"deletion_intervals":intervals}),
        )
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sleeper(path: &Path) -> Process {
        Process::spawn(Command::new("/usr/bin/sleep").arg("30"), path).unwrap()
    }
    fn gone(pid: Pid) -> bool {
        !Path::new(&format!("/proc/{}", pid.as_raw_nonzero())).exists()
    }
    #[test]
    fn drop_reaps_owned_leader_and_preserves_foreign_process() {
        let root = tempfile::tempdir().unwrap();
        let foreign = sleeper(&root.path().join("foreign"));
        let owned = sleeper(&root.path().join("owned"));
        let pid = Pid::from_child(owned.child.as_ref().unwrap());
        drop(owned);
        assert!(gone(pid));
        assert!(!foreign.exited().unwrap());
    }
    #[test]
    fn early_spawn_failure_preserves_foreign_process() {
        let root = tempfile::tempdir().unwrap();
        let foreign = sleeper(&root.path().join("foreign"));
        assert!(Process::spawn(
            &mut Command::new(root.path().join("missing-provider")),
            &root.path().join("failed")
        )
        .is_err());
        assert!(!foreign.exited().unwrap());
    }
    #[test]
    fn deadline_rejection_retains_cleanup_authority() {
        let root = tempfile::tempdir().unwrap();
        let foreign = sleeper(&root.path().join("foreign"));
        let mut owned = sleeper(&root.path().join("owned"));
        let pid = Pid::from_child(owned.child.as_ref().unwrap());
        owned.deadline = Instant::now();
        assert!(owned
            .drain()
            .unwrap_err()
            .to_string()
            .contains("deadline exceeded"));
        drop(owned);
        assert!(gone(pid));
        assert!(!foreign.exited().unwrap());
    }
}
