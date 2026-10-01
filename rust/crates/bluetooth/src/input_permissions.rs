use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
};
use std::{
    fs::File,
    io::{self, Read},
    os::{fd::OwnedFd, unix::process::ExitStatusExt},
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

#[derive(Debug, thiserror::Error)]
pub enum PermissionError {
    #[error("{}", crate::input::io_message(.0, None))]
    Io(#[from] io::Error),
    #[error("{}", crate::input::io_message(.0, Some(Path::new("sudo"))))]
    Spawn(#[source] io::Error),
    #[error("Command '{command}' timed out after 3 seconds")]
    Timeout { command: String },
    #[error("Command '{command}' returned non-zero exit status {code}.")]
    Exit { command: String, code: i32 },
    #[error("Command '{command}' died with {}.", signal_description(*.signal))]
    Signal { command: String, signal: i32 },
}

fn signal_description(raw: i32) -> String {
    match nix::sys::signal::Signal::try_from(raw) {
        Ok(signal) => format!("<Signals.{signal:?}: {raw}>"),
        Err(_) if raw == libc::SIGRTMIN() => format!("<Signals.SIGRTMIN: {raw}>"),
        Err(_) if raw == libc::SIGRTMAX() => format!("<Signals.SIGRTMAX: {raw}>"),
        Err(_) => format!("unknown signal {raw}"),
    }
}

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        match self.0.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => {}
            Err(error) => eprintln!("Bluetooth permission child status: {error}"),
        }
        if let Err(error) = self.0.kill() {
            eprintln!("Bluetooth permission child kill: {error}");
        }
        if let Err(error) = self.0.wait() {
            eprintln!("Bluetooth permission child reap: {error}");
        }
    }
}

struct Pipe {
    file: File,
    eof: bool,
}
impl Pipe {
    fn new(fd: OwnedFd) -> io::Result<Self> {
        fcntl_setfl(&fd, fcntl_getfl(&fd)? | OFlags::NONBLOCK)?;
        Ok(Self {
            file: fd.into(),
            eof: false,
        })
    }

    fn drain(&mut self) -> io::Result<()> {
        let mut buffer = [0; 16384];
        match self.file.read(&mut buffer) {
            Ok(0) => self.eof = true,
            Ok(_) => {}
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

fn capture(child: &mut Child, command: &str) -> Result<ExitStatus, PermissionError> {
    let started = Instant::now();
    let mut stdout = Pipe::new(
        child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("permission stdout missing"))?
            .into(),
    )?;
    let mut stderr = Pipe::new(
        child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("permission stderr missing"))?
            .into(),
    )?;
    loop {
        let remaining = Duration::from_secs(3)
            .checked_sub(started.elapsed())
            .ok_or_else(|| PermissionError::Timeout {
                command: command.to_owned(),
            })?;
        stdout.drain()?;
        stderr.drain()?;
        if let Some(status) = child.try_wait()? {
            if stdout.eof && stderr.eof {
                return Ok(status);
            }
        }
        let mut fds = Vec::with_capacity(2);
        if !stdout.eof {
            fds.push(PollFd::new(&stdout.file, PollFlags::IN));
        }
        if !stderr.eof {
            fds.push(PollFd::new(&stderr.file, PollFlags::IN));
        }
        let timeout = Timespec::try_from(remaining.min(Duration::from_millis(10)))
            .map_err(io::Error::other)?;
        match poll(&mut fds, Some(&timeout)) {
            Ok(_) | Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(io::Error::from(error).into()),
        }
    }
}

pub(crate) fn grant(path: &Path) -> Result<(), PermissionError> {
    for args in [["-n", "chgrp", "gpio"], ["-n", "chmod", "g+rw"]] {
        let command = format!(
            "['sudo', '-n', '{}', '{}', '{}']",
            args[1],
            args[2],
            path.display()
        );
        let child = Command::new("sudo")
            .args(args)
            .arg(path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(PermissionError::Spawn)?;
        let mut child = OwnedChild(child);
        let status = capture(&mut child.0, &command)?;
        if let Some(signal) = status.signal() {
            return Err(PermissionError::Signal { command, signal });
        }
        if !status.success() {
            return Err(PermissionError::Exit {
                command,
                code: status.code().unwrap_or(1),
            });
        }
    }
    Ok(())
}
