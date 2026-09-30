use openpilot_process_supervision::CapturedCommand;
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
};
use std::{
    fs::File,
    io::{self, Read},
    os::fd::OwnedFd,
    path::Path,
    process::{Child, Output},
    time::{Duration, Instant},
};

struct Pipe {
    file: File,
    bytes: Vec<u8>,
    eof: bool,
}

impl Pipe {
    fn new(fd: OwnedFd) -> io::Result<Self> {
        let flags = fcntl_getfl(&fd)?;
        fcntl_setfl(&fd, flags | OFlags::NONBLOCK)?;
        Ok(Self {
            file: fd.into(),
            bytes: Vec::new(),
            eof: false,
        })
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

fn capture(child: &mut Child) -> io::Result<Output> {
    let started = Instant::now();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("missing Git stdout pipe"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("missing Git stderr pipe"))?;
    let mut stdout = Pipe::new(stdout.into())?;
    let mut stderr = Pipe::new(stderr.into())?;
    loop {
        let remaining = Duration::from_secs(1)
            .checked_sub(started.elapsed())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::TimedOut, "Git identity read timed out")
            })?;
        stdout.read()?;
        stderr.read()?;
        if let Some(status) = child.try_wait()? {
            if stdout.eof && stderr.eof {
                return Ok(Output {
                    status,
                    stdout: stdout.bytes,
                    stderr: stderr.bytes,
                });
            }
        }
        let mut descriptors = Vec::with_capacity(2);
        if !stdout.eof {
            descriptors.push(PollFd::new(&stdout.file, PollFlags::IN));
        }
        if !stderr.eof {
            descriptors.push(PollFd::new(&stderr.file, PollFlags::IN));
        }
        let timeout = Timespec::try_from(remaining.min(Duration::from_millis(10)))
            .map_err(io::Error::other)?;
        match poll(&mut descriptors, Some(&timeout)) {
            Ok(_) | Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(error.into()),
        }
    }
}

fn run(repo: &Path, launcher: &Path) -> io::Result<Output> {
    let mut child = CapturedCommand {
        launcher: launcher.into(),
        cwd: repo.into(),
        argv: [
            "git",
            "--no-optional-locks",
            "rev-parse",
            "--verify",
            "HEAD^{commit}",
        ]
        .into_iter()
        .map(Into::into)
        .collect(),
    }
    .spawn()
    .map_err(io::Error::other)?;
    let result = capture(&mut child.process);
    let termination = if result.is_err() {
        child.process.kill()
    } else {
        Ok(())
    };
    child.process.wait()?;
    match termination {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::InvalidInput => {}
        Err(error) => return Err(error),
    }
    result
}

pub(crate) fn git_commit(repo: &Path, launcher: &Path) -> io::Result<String> {
    let output = run(repo, launcher)?;
    if !output.status.success() {
        return Err(io::Error::other("Git identity command failed"));
    }
    std::str::from_utf8(&output.stderr)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let text = String::from_utf8(output.stdout)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(text
        .trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
        .into())
}
