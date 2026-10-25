//! Narrow length-prefixed peer, using the existing QA unreaped-group owner policy.
//! Original queue/response reads have no deadline; cancellation stays responsive.
//! Only the original final ten-second wait is an acceptance deadline.
use super::Result;
use rustix::{
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
    process::{kill_process_group, waitid, Pid, Signal, WaitId, WaitIdOptions},
};
use std::{
    ffi::OsString,
    io::{Read, Write},
    os::unix::process::CommandExt,
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
pub struct Peer<'a> {
    child: Child,
    pid: Pid,
    input: Option<ChildStdin>,
    output: Option<ChildStdout>,
    stop: &'a AtomicBool,
}
impl<'a> Peer<'a> {
    pub fn start(binary: &Path, args: &[OsString], stop: &'a AtomicBool) -> Result<Self> {
        check(stop)?;
        let child = Command::new(binary)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .process_group(0)
            .spawn()?;
        let pid = Pid::from_child(&child);
        // Ownership precedes every fallible descriptor operation.
        let mut owner = Self {
            child,
            pid,
            input: None,
            output: None,
            stop,
        };
        owner.input = owner.child.stdin.take();
        owner.output = owner.child.stdout.take();
        let output = owner.output.as_ref().ok_or("procLog stdout unavailable")?;
        fcntl_setfl(output, fcntl_getfl(output)? | OFlags::NONBLOCK)?;
        let input = owner.input.as_ref().ok_or("procLog stdin unavailable")?;
        fcntl_setfl(input, fcntl_getfl(input)? | OFlags::NONBLOCK)?;
        Ok(owner)
    }
    pub fn send(&mut self, timestamp: u64) -> Result<()> {
        let bytes = format!("{timestamp}\n").into_bytes();
        let mut offset = 0;
        while offset < bytes.len() {
            check(self.stop)?;
            match self
                .input
                .as_mut()
                .ok_or("procLog stdin closed")?
                .write(&bytes[offset..])
            {
                Ok(0) => return Err("procLog input closed during request".into()),
                Ok(n) => offset += n,
                Err(e) if matches!(e.kind(), std::io::ErrorKind::Interrupted) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(2))
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
    pub fn read(&mut self, length: usize) -> Result<Vec<u8>> {
        let mut bytes = vec![0; length];
        let mut offset = 0;
        while offset < length {
            check(self.stop)?;
            match self
                .output
                .as_mut()
                .ok_or("procLog stdout closed")?
                .read(&mut bytes[offset..])
            {
                Ok(0) => break,
                Ok(n) => offset += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(2))
                }
                Err(e) => return Err(e.into()),
            }
        }
        bytes.truncate(offset);
        Ok(bytes)
    }
    pub fn frame(&mut self) -> Result<Vec<u8>> {
        let prefix = self.read(8)?;
        let mut word = [0; 8];
        word[..prefix.len()].copy_from_slice(&prefix);
        let size = u64::from_le_bytes(word);
        if size == 0 || size >= 1_000_000 {
            return Err(format!("invalid procLog frame size {size}").into());
        }
        self.read(size as usize)
    }
    pub fn finish(&mut self) -> Result<()> {
        self.input = None;
        self.wait(Some(Duration::from_secs(10)))
    }
    fn wait(&mut self, timeout: Option<Duration>) -> Result<()> {
        let start = Instant::now();
        loop {
            check(self.stop)?;
            match waitid(
                WaitId::Pid(self.pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            ) {
                Ok(Some(status)) => {
                    return if status.exit_status() == Some(0) {
                        Ok(())
                    } else {
                        Err("procLog candidate exited unsuccessfully".into())
                    }
                }
                Ok(None) => {}
                Err(rustix::io::Errno::INTR) => continue,
                Err(e) => return Err(e.into()),
            }
            if timeout.is_some_and(|limit| start.elapsed() >= limit) {
                return Err("procLog original final wait exceeded ten seconds".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    pub fn queue(binary: &Path, stop: &AtomicBool) -> Result<Vec<u8>> {
        let mut peer = Peer::start(binary, &["--queue-size".into()], stop)?;
        peer.input = None;
        let mut out = vec![];
        loop {
            let data = peer.read(4096)?;
            let done = data.len() < 4096;
            out.extend_from_slice(&data);
            if done {
                break;
            }
        }
        peer.wait(None)?;
        Ok(out)
    }
}
impl Drop for Peer<'_> {
    fn drop(&mut self) {
        // waitid(NOWAIT) above retains leader identity until group cleanup ends.
        let _ = kill_process_group(self.pid, Signal::KILL);
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
pub fn check(stop: &AtomicBool) -> Result<()> {
    if stop.load(Ordering::SeqCst) {
        Err("procLog reference interrupted".into())
    } else {
        Ok(())
    }
}
