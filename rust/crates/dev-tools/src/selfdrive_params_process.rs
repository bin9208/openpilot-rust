//! Interactive QA child ownership; the unreaped leader prevents process-group ID reuse.
use crate::seal::Result;
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
    process::{kill_process_group, waitid, Pid, Signal, WaitId, WaitIdOptions},
};
use std::os::unix::process::CommandExt;
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::{Duration, Instant},
};
pub struct Process {
    child: Child,
    pid: Pid,
    input: Option<ChildStdin>,
    output: ChildStdout,
    pending: Vec<u8>,
    reaped: bool,
}
impl Process {
    pub fn start(binary: &Path, args: &[&std::ffi::OsStr], stderr: File) -> Result<Self> {
        let mut command = Command::new(binary);
        command
            .args(args)
            .env("OPENPILOT_PREFIX", "fixture")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr))
            .process_group(0);
        let mut child = command.spawn()?;
        let Some(pid) = Pid::from_raw(child.id() as i32) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err("invalid child PID".into());
        };
        let streams = (|| -> Result<_> {
            Ok((
                child.stdin.take().ok_or("missing child input")?,
                child.stdout.take().ok_or("missing child output")?,
            ))
        })();
        let (input, output) = match streams {
            Ok(streams) => streams,
            Err(error) => {
                let _ = kill_process_group(pid, Signal::KILL);
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        let mut process = Self {
            child,
            pid,
            input: Some(input),
            output,
            pending: vec![],
            reaped: false,
        };
        fcntl_setfl(
            &process.output,
            fcntl_getfl(&process.output)? | OFlags::NONBLOCK,
        )?;
        let input = process.input.as_ref().ok_or("missing child input")?;
        fcntl_setfl(input, fcntl_getfl(input)? | OFlags::NONBLOCK)?;
        if process.line()?.trim_matches(|character: char| {
            character.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&character)
        }) != "ready"
        {
            return Err("child readiness differs".into());
        }
        Ok(process)
    }
    pub fn line(&mut self) -> Result<String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(index) = self.pending.iter().position(|b| matches!(b, b'\n' | b'\r')) {
                // TextIOWrapper's universal-newline decoder holds a terminal
                // CR until the following byte or EOF, then consumes CRLF once.
                if self.pending[index] == b'\n' || index + 1 < self.pending.len() {
                    let length = index
                        + if self.pending.get(index..index + 2) == Some(b"\r\n") {
                            2
                        } else {
                            1
                        };
                    let line = String::from_utf8(self.pending[..index].to_vec())?;
                    self.pending.drain(..length);
                    return Ok(line);
                }
            }
            if Instant::now() >= deadline {
                return Err("child line timed out".into());
            }
            let mut buffer = [0; 4096];
            match self.output.read(&mut buffer) {
                Ok(0) => {
                    if self.pending.is_empty() {
                        return Err("child output ended before response".into());
                    }
                    if self.pending.last() == Some(&b'\r') {
                        self.pending.pop();
                    }
                    return Ok(String::from_utf8(std::mem::take(&mut self.pending))?);
                }
                Ok(count) => self.pending.extend_from_slice(&buffer[..count]),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    let mut fds = [PollFd::new(&self.output, PollFlags::IN)];
                    match poll(
                        &mut fds,
                        Some(&Timespec {
                            tv_sec: 0,
                            tv_nsec: 10_000_000,
                        }),
                    ) {
                        Ok(_) | Err(rustix::io::Errno::INTR) => {}
                        Err(error) => return Err(error.into()),
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    pub fn request(&mut self, line: &str) -> Result<String> {
        let input = self.input.as_mut().ok_or("child input already closed")?;
        let mut bytes = line.as_bytes().to_vec();
        bytes.push(b'\n');
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut offset = 0;
        while offset < bytes.len() {
            if Instant::now() >= deadline {
                return Err("child request timed out".into());
            }
            match input.write(&bytes[offset..]) {
                Ok(0) => return Err("child input closed during request".into()),
                Ok(count) => offset += count,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    let mut fds = [PollFd::new(&*input, PollFlags::OUT)];
                    match poll(
                        &mut fds,
                        Some(&Timespec {
                            tv_sec: 0,
                            tv_nsec: 10_000_000,
                        }),
                    ) {
                        Ok(_) | Err(rustix::io::Errno::INTR) => {}
                        Err(error) => return Err(error.into()),
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
        self.line()
    }
    pub fn finish(&mut self) -> Result<()> {
        self.input = None;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match waitid(
                WaitId::Pid(self.pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            ) {
                Ok(Some(_)) => break,
                Ok(None) => {}
                Err(rustix::io::Errno::INTR) => {}
                Err(error) => return Err(error.into()),
            }
            if Instant::now() >= deadline {
                return Err("child exit timed out".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        match kill_process_group(self.pid, Signal::KILL) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => {}
            Err(error) => return Err(error.into()),
        }
        let status = self.child.wait()?;
        self.reaped = true;
        if !status.success() {
            return Err(format!("child exited {status}").into());
        }
        Ok(())
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = kill_process_group(self.pid, Signal::KILL);
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn descendant_stopped(stat: std::io::Result<String>) -> std::io::Result<bool> {
        match stat {
            Ok(stat) => Ok(stat
                .rsplit_once(") ")
                .is_some_and(|(_, fields)| fields.starts_with('Z'))),
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    || error.raw_os_error() == Some(rustix::io::Errno::SRCH.raw_os_error()) =>
            {
                Ok(true)
            }
            Err(error) => Err(error),
        }
    }
    #[test]
    fn readiness_request_eof_and_reap_preserve_interactive_order() {
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("peer");
        std::fs::write(&binary,b"#!/bin/sh\nprintf 'ready\\n'\nwhile IFS= read -r line; do printf '%s\\n' \"$line\"; done\n").unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut child = Process::start(
            &binary,
            &[],
            File::create(temp.path().join("stderr")).unwrap(),
        )
        .unwrap();
        assert_eq!(child.request("one").unwrap(), "one");
        assert_eq!(child.request("two").unwrap(), "two");
        child.finish().unwrap();
        assert!(child.reaped);
    }
    #[test]
    fn wrong_ready_and_early_eof_fail_without_success() {
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("peer");
        for content in [
            b"#!/bin/sh\nprintf 'wrong\\n'\n".as_slice(),
            b"#!/bin/sh\nexit 7\n".as_slice(),
        ] {
            std::fs::write(&binary, content).unwrap();
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
            assert!(Process::start(
                &binary,
                &[],
                File::create(temp.path().join("stderr")).unwrap()
            )
            .is_err());
        }
    }
    #[test]
    fn universal_newlines_and_unterminated_final_response_match_textio() {
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("peer");
        std::fs::write(&binary, b"#!/bin/sh\nprintf 'ready\\n'\nIFS= read -r line\nprintf 'first\\rsecond\\r\\nthird'\n").unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut child = Process::start(
            &binary,
            &[],
            File::create(temp.path().join("stderr")).unwrap(),
        )
        .unwrap();
        assert_eq!(child.request("request").unwrap(), "first");
        assert_eq!(child.line().unwrap(), "second");
        assert_eq!(child.line().unwrap(), "third");
        child.finish().unwrap();
    }
    #[test]
    fn descendant_observation_accepts_only_dead_or_missing_processes() {
        for stat in ["12 (sleep) Z 1", "12 (sleep (child)) Z 1"] {
            assert!(descendant_stopped(Ok(stat.to_owned())).unwrap());
        }
        for stat in ["12 (sleep) S 1", "12 (sleep) R 1", "malformed"] {
            assert!(!descendant_stopped(Ok(stat.to_owned())).unwrap());
        }
        for errno in [rustix::io::Errno::NOENT, rustix::io::Errno::SRCH] {
            assert!(descendant_stopped(Err(std::io::Error::from_raw_os_error(
                errno.raw_os_error(),
            )))
            .unwrap());
        }
        for errno in [rustix::io::Errno::ACCES, rustix::io::Errno::IO] {
            let error =
                descendant_stopped(Err(std::io::Error::from_raw_os_error(errno.raw_os_error())))
                    .unwrap_err();
            assert_eq!(error.raw_os_error(), Some(errno.raw_os_error()));
        }
    }
    #[test]
    fn finish_and_error_drop_kill_descendants_before_reaping_the_leader() {
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("peer");
        std::fs::write(
            &binary,
            b"#!/bin/sh\nsleep 30 &\necho $! > \"$1\"\nprintf 'ready\\n'\nIFS= read -r line\nexit 0\n",
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        for finish in [false, true] {
            let descendant_file = temp
                .path()
                .join(if finish { "finished" } else { "dropped" });
            let mut child = Process::start(
                &binary,
                &[descendant_file.as_os_str()],
                File::create(temp.path().join("stderr")).unwrap(),
            )
            .unwrap();
            let leader = child.pid;
            let descendant: u32 = std::fs::read_to_string(&descendant_file)
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            if finish {
                child.finish().unwrap();
            }
            drop(child);
            assert_eq!(
                waitid(
                    WaitId::Pid(leader),
                    WaitIdOptions::EXITED | WaitIdOptions::NOHANG
                )
                .unwrap_err(),
                rustix::io::Errno::CHILD
            );
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                let stopped =
                    descendant_stopped(std::fs::read_to_string(format!("/proc/{descendant}/stat")))
                        .unwrap_or_else(|error| panic!("{error}"));
                if stopped {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "owned descendant {descendant} still runs"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}
