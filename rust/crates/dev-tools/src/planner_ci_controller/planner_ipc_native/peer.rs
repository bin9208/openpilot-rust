//! Planner 전용 C++ peer의 raw-file protocol. Python worker나 공통 process framework는 없다.
use crate::seal::Result;
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
};
use std::{
    fs::File,
    io::{self, Read, Write},
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::{Duration, Instant},
};

pub fn hex(path: &Path) -> Result<String> {
    Ok(path
        .to_str()
        .ok_or("peer path must be UTF-8")?
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
fn unhex(value: &str) -> Result<PathBuf> {
    if !value.len().is_multiple_of(2)
        || value.len() > 8192
        || !value.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("invalid peer path hex".into());
    }
    let bytes = (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if bytes.contains(&0) {
        return Err("NUL peer path".into());
    }
    Ok(String::from_utf8(bytes)?.into())
}
pub struct Peer {
    child: Child,
    input: Option<ChildStdin>,
    output: ChildStdout,
    pending: Vec<u8>,
    finished: bool,
}

impl Peer {
    pub fn start(binary: &Path, prefix: &str, stderr: &Path, deadline: Instant) -> Result<Self> {
        let mut child = Command::new(binary)
            .env("OPENPILOT_PREFIX", prefix)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(File::create(stderr)?))
            .spawn()?;
        let (input, output) = match (child.stdin.take(), child.stdout.take()) {
            (Some(input), Some(output)) => (input, output),
            (input, _) => {
                let _owner = super::ChildOwner {
                    child,
                    reaped: false,
                };
                return Err(if input.is_none() {
                    "missing C++ peer stdin"
                } else {
                    "missing C++ peer stdout"
                }
                .into());
            }
        };
        let mut peer = Self {
            child,
            input: Some(input),
            output,
            pending: Vec::new(),
            finished: false,
        };
        let input = peer.input.as_ref().ok_or("missing C++ peer stdin")?;
        fcntl_setfl(input, fcntl_getfl(input)? | OFlags::NONBLOCK)?;
        fcntl_setfl(&peer.output, fcntl_getfl(&peer.output)? | OFlags::NONBLOCK)?;
        if peer.line(deadline)? != "READY" {
            return Err("C++ planner peer readiness differs".into());
        }
        Ok(peer)
    }
    fn wait(fd: &impl std::os::fd::AsFd, reading: bool, deadline: Instant) -> Result<()> {
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or("C++ peer command deadline")?;
            let time = Timespec {
                tv_sec: remaining.as_secs().try_into()?,
                tv_nsec: remaining.subsec_nanos().into(),
            };
            match poll(
                &mut [PollFd::new(
                    fd,
                    if reading {
                        PollFlags::IN
                    } else {
                        PollFlags::OUT
                    },
                )],
                Some(&time),
            ) {
                Ok(0) => return Err("C++ peer command timed out".into()),
                Ok(_) => return Ok(()),
                Err(rustix::io::Errno::INTR) => continue,
                Err(error) => return Err(error.into()),
            }
        }
    }
    fn send(&mut self, command: &str, deadline: Instant) -> Result<()> {
        if command.contains(['\n', '\r']) || command.len() > 16384 {
            return Err("invalid peer command".into());
        }
        let input = self.input.as_mut().ok_or("C++ peer input closed")?;
        let line = format!("{command}\n");
        let mut bytes = line.as_bytes();
        while !bytes.is_empty() {
            Self::wait(input, false, deadline)?;
            match input.write(bytes) {
                Ok(0) => return Err("C++ peer input ended".into()),
                Ok(n) => bytes = &bytes[n..],
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    continue
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
    fn line(&mut self, deadline: Instant) -> Result<String> {
        loop {
            if let Some(i) = self.pending.iter().position(|b| *b == b'\n') {
                let line = self.pending.drain(..=i).collect::<Vec<_>>();
                return Ok(String::from_utf8(line[..i].to_vec())?);
            }
            if self.pending.len() > 16384 {
                return Err("C++ peer response line exceeds bound".into());
            }
            Self::wait(&self.output, true, deadline)?;
            let mut bytes = [0; 4096];
            match self.output.read(&mut bytes) {
                Ok(0) => return Err("C++ peer EOF".into()),
                Ok(n) => self.pending.extend_from_slice(&bytes[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    continue
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
    pub fn request(&mut self, command: &str, deadline: Instant) -> Result<String> {
        self.send(command, deadline)?;
        self.line(deadline)
    }
    pub fn ok(&mut self, command: &str, deadline: Instant) -> Result<()> {
        if self.request(command, deadline)? != "OK" {
            return Err("C++ peer command rejected".into());
        }
        Ok(())
    }
    pub fn publish(&mut self, service: &str, path: &Path, deadline: Instant) -> Result<()> {
        self.ok(&format!("publish {service} {}", hex(path)?), deadline)
    }
    pub fn drain(&mut self, output: &Path, deadline: Instant) -> Result<Vec<(String, PathBuf)>> {
        self.send(&format!("drain {}", hex(output)?), deadline)?;
        let mut packets = Vec::new();
        loop {
            let line = self.line(deadline)?;
            if line == "END" {
                return Ok(packets);
            }
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() != 3 || fields[0] != "PACKET" || packets.len() >= 4096 {
                return Err("invalid C++ publication roster".into());
            }
            let path = unhex(fields[2])?;
            if !path.starts_with(output) || path.canonicalize()? != path {
                return Err("C++ publication escaped output".into());
            }
            packets.push((fields[1].into(), path));
        }
    }
    pub fn finish<T>(&mut self, work: Result<T>) -> Result<T> {
        let deadline = Instant::now() + Duration::from_secs(3);
        let close = if work.is_ok() {
            self.ok("stop", deadline)
        } else {
            self.child.kill().map_err(Into::into)
        };
        self.input.take();
        if close.is_err() {
            let _ = self.child.kill();
        }
        loop {
            if let Some(status) = self.child.try_wait()? {
                self.finished = true;
                return match work {
                    Err(error) => Err(error),
                    Ok(value) => {
                        close?;
                        if !status.success() {
                            return Err(format!(
                                "C++ peer exited {}",
                                status
                                    .code()
                                    .unwrap_or_else(|| -status.signal().unwrap_or(1))
                            )
                            .into());
                        }
                        Ok(value)
                    }
                };
            }
            if Instant::now() >= deadline {
                let _ = self.child.kill();
                return Err("C++ peer close exceeded 3 seconds".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        if !self.finished {
            let deadline = Instant::now() + Duration::from_secs(3);
            let _ = self.child.kill();
            loop {
                match self.child.try_wait() {
                    Ok(Some(_)) => {
                        self.finished = true;
                        break;
                    }
                    Ok(None) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    _ => break,
                }
            }
        }
    }
}
