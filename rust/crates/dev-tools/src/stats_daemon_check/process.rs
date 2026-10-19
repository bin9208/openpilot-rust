//! Real interactive children. Only the outer worker owns a process group.
use crate::seal::Result;
use rustix::{
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
    process::{kill_process_group, waitid, Pid, Signal, WaitId, WaitIdOptions},
};
use std::{
    io::{Read, Write},
    os::unix::process::CommandExt,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
pub(crate) struct Process<'a> {
    child: Child,
    pid: Pid,
    input: Option<ChildStdin>,
    output: Option<ChildStdout>,
    buffer: Vec<u8>,
    group: bool,
    cancel: &'a AtomicBool,
}
impl<'a> Process<'a> {
    pub(super) fn spawn(
        command: &mut Command,
        cancel: &'a AtomicBool,
        group: bool,
    ) -> Result<Self> {
        Self::spawn_inner(command, cancel, group, true)
    }
    pub(crate) fn spawn_logged(
        command: &mut Command,
        cancel: &'a AtomicBool,
        group: bool,
    ) -> Result<Self> {
        Self::spawn_inner(command, cancel, group, false)
    }
    pub(crate) fn pid(&self) -> Pid {
        self.pid
    }
    fn spawn_inner(
        command: &mut Command,
        cancel: &'a AtomicBool,
        group: bool,
        capture: bool,
    ) -> Result<Self> {
        check(cancel)?;
        command.stdin(Stdio::piped());
        if capture {
            command.stdout(Stdio::piped());
        }
        if group {
            command.process_group(0);
        }
        let mut child = command.spawn()?;
        let pid = Pid::from_child(&child);
        let input = child.stdin.take();
        let output = child.stdout.take();
        let result = Self {
            child,
            pid,
            input,
            output,
            buffer: vec![],
            group,
            cancel,
        };
        if let Some(output) = &result.output {
            fcntl_setfl(output, fcntl_getfl(output)? | OFlags::NONBLOCK)?;
        }
        if let Some(input) = &result.input {
            fcntl_setfl(input, fcntl_getfl(input)? | OFlags::NONBLOCK)?;
        }
        Ok(result)
    }
    pub(super) fn send(&mut self, text: &str) -> Result<()> {
        let mut bytes = text.as_bytes();
        while !bytes.is_empty() {
            check(self.cancel)?;
            match self
                .input
                .as_mut()
                .ok_or("closed child input")?
                .write(bytes)
            {
                Ok(0) => return Err("child input closed".into()),
                Ok(n) => bytes = &bytes[n..],
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => (),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(2))
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
    fn receive(&mut self) -> Result<Option<bool>> {
        check(self.cancel)?;
        let mut bytes = [0; 8192];
        match self
            .output
            .as_mut()
            .ok_or("child stdout is file-backed")?
            .read(&mut bytes)
        {
            Ok(0) => Ok(Some(false)),
            Ok(n) => {
                self.buffer.extend_from_slice(&bytes[..n]);
                Ok(Some(true))
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                Ok(None)
            }
            Err(e) => Err(e.into()),
        }
    }
    pub(super) fn line(&mut self, first_byte_timeout: Option<Duration>) -> Result<String> {
        self.line_or_eof(first_byte_timeout)?
            .ok_or_else(|| "child clock/output closed".into())
    }
    pub(super) fn line_or_eof(
        &mut self,
        first_byte_timeout: Option<Duration>,
    ) -> Result<Option<String>> {
        let start = Instant::now();
        loop {
            if let Some(index) = self.buffer.iter().position(|b| *b == b'\n') {
                let line = self.buffer.drain(..=index).collect::<Vec<_>>();
                return Ok(Some(
                    String::from_utf8(line)?
                        .trim_matches(|c: char| c.is_whitespace() || matches!(c, '\x1c'..='\x1f'))
                        .into(),
                ));
            }
            // Original select bounds readiness only; readline can then await the newline.
            if self.buffer.is_empty()
                && first_byte_timeout.is_some_and(|limit| start.elapsed() >= limit)
            {
                return Err("child clock readiness timeout".into());
            }
            match self.receive()? {
                Some(false) if self.buffer.is_empty() => return Ok(None),
                Some(false) => {
                    return Ok(Some(
                        String::from_utf8(std::mem::take(&mut self.buffer))?
                            .trim_matches(|c: char| {
                                c.is_whitespace() || matches!(c, '\x1c'..='\x1f')
                            })
                            .into(),
                    ))
                }
                Some(true) => (),
                None => std::thread::sleep(Duration::from_millis(2)),
            }
        }
    }
    pub(super) fn output(&mut self) -> Result<Vec<u8>> {
        loop {
            match self.receive()? {
                Some(false) => return Ok(std::mem::take(&mut self.buffer)),
                Some(true) => (),
                None => std::thread::sleep(Duration::from_millis(2)),
            }
        }
    }
    pub(super) fn close_input(&mut self) {
        self.input.take();
    }
    pub(crate) fn exited(&self) -> Result<Option<i32>> {
        match waitid(
            WaitId::Pid(self.pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        ) {
            Ok(Some(s)) => Ok(Some(
                s.exit_status()
                    .unwrap_or_else(|| -s.terminating_signal().unwrap_or(0)),
            )),
            Ok(None) => Ok(None),
            Err(rustix::io::Errno::INTR) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    pub(crate) fn wait(&self, timeout: Option<Duration>) -> Result<i32> {
        let start = Instant::now();
        loop {
            check(self.cancel)?;
            if let Some(code) = self.exited()? {
                return Ok(code);
            }
            if timeout.is_some_and(|limit| start.elapsed() >= limit) {
                return Err("child final wait timeout".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    pub(crate) fn signal(&self, signal: Signal) -> Result<()> {
        rustix::process::kill_process(self.pid, signal)?;
        Ok(())
    }
}
impl Drop for Process<'_> {
    fn drop(&mut self) {
        if self.group {
            let _ = kill_process_group(self.pid, Signal::KILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
pub(super) fn check(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::SeqCst) {
        Err("statistics comparison cancelled".into())
    } else {
        Ok(())
    }
}
