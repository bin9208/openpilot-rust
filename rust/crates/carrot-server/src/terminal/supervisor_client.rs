//! A private child process owns adoption; the multithreaded web server does not.
use super::Config;
use std::{
    cell::RefCell,
    fs::File,
    io::{self, BufRead, BufReader, Write},
    os::unix::net::{UnixListener, UnixStream},
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

pub(super) struct Supervisor {
    _child: Helper,
    control: RefCell<BufReader<UnixStream>>,
    _directory: tempfile::TempDir,
}
impl Supervisor {
    pub fn start(
        config: &Config,
        slave: File,
        command: &str,
    ) -> Result<(Self, rustix::process::Pid), crate::Error> {
        Self::start_until(
            config,
            slave,
            command,
            Instant::now() + Duration::from_secs(10),
        )
    }
    fn start_until(
        config: &Config,
        slave: File,
        command: &str,
        deadline: Instant,
    ) -> Result<(Self, rustix::process::Pid), crate::Error> {
        let directory = tempfile::Builder::new().prefix("carrot-pty-").tempdir()?;
        let path = directory.path().join("control");
        let listener = UnixListener::bind(&path)?;
        listener.set_nonblocking(true)?;
        let executable = config.launcher.with_file_name("openpilot-carrot-server");
        let child = Command::new(&executable)
            .arg("--terminal-supervisor")
            .arg(&config.launcher)
            .arg(&path)
            .arg(Config::shell())
            .arg(command)
            .env_remove("TMUX")
            .envs(Config::shell_environment())
            .stdin(Stdio::from(slave.try_clone()?))
            .stdout(Stdio::from(slave.try_clone()?))
            .stderr(Stdio::from(slave))
            .spawn()
            .map_err(|error| crate::state::io_error(error, &executable))?;
        let mut child = Helper(Some(child));
        let accepted = (|| -> io::Result<UnixStream> {
            let mut next_resume = Instant::now();
            loop {
                match listener.accept() {
                    Ok((stream, _)) => return Ok(stream),
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        if child.0.as_mut().unwrap().try_wait()?.is_some() {
                            return Err(io::Error::other(
                                "terminal supervisor exited before handshake",
                            ));
                        }
                        let now = Instant::now();
                        if now >= deadline {
                            return Err(io::Error::new(
                                io::ErrorKind::TimedOut,
                                "terminal supervisor handshake",
                            ));
                        }
                        if now >= next_resume {
                            resume(child.0.as_ref().unwrap())?;
                            next_resume = now + Duration::from_millis(20);
                        }
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    Err(error) => return Err(error),
                }
            }
        })();
        // Close queued, unaccepted control streams before the child's guard
        // resumes/reaps it on every startup error, including timeout.
        drop(listener);
        let stream = accepted?;
        let supervisor = Self {
            _child: child,
            control: RefCell::new(BufReader::new(stream)),
            _directory: directory,
        };
        supervisor
            .control
            .borrow()
            .get_ref()
            .set_read_timeout(Some(Duration::from_secs(5)))?;
        supervisor
            .control
            .borrow()
            .get_ref()
            .set_write_timeout(Some(Duration::from_secs(5)))?;
        let greeting = supervisor.reply()?;
        if let Some(errno) = greeting
            .strip_prefix("ERRNO ")
            .and_then(|raw| raw.parse().ok())
        {
            return Err(crate::state::io_error(
                io::Error::from_raw_os_error(errno),
                std::path::Path::new(&Config::shell()),
            ));
        }
        let pid = greeting
            .strip_prefix("PID ")
            .and_then(|pid| pid.parse().ok())
            .and_then(rustix::process::Pid::from_raw)
            .ok_or_else(|| io::Error::other(greeting))?;
        Ok((supervisor, pid))
    }
    fn reply(&self) -> io::Result<String> {
        let mut line = String::new();
        self.control.borrow_mut().read_line(&mut line)?;
        if line.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "terminal supervisor closed",
            ));
        }
        Ok(line.trim_end().to_owned())
    }
    fn request(&self, command: u8) -> io::Result<String> {
        self.control.borrow_mut().get_mut().write_all(&[command])?;
        self.reply()
    }
    pub fn observe(&self) -> io::Result<Option<i32>> {
        let response = self.request(b'?')?;
        if response == "RUNNING" {
            return Ok(None);
        }
        response
            .strip_prefix("EXIT ")
            .and_then(|code| code.parse().ok())
            .map(Some)
            .ok_or_else(|| io::Error::other(response))
    }
    pub fn signal(&self, signal: rustix::process::Signal) -> io::Result<()> {
        let response = self.request(u8::try_from(signal.as_raw()).map_err(io::Error::other)?)?;
        if response == "OK" {
            Ok(())
        } else {
            Err(io::Error::other(response))
        }
    }
}
// Keep child identity reserved through all signals. Only this owner reaps it.
struct Helper(Option<Child>);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

fn resume(child: &Child) -> io::Result<()> {
    let pid = rustix::process::Pid::from_raw(i32::try_from(child.id()).map_err(io::Error::other)?)
        .ok_or_else(|| io::Error::other("invalid terminal supervisor PID"))?;
    match rustix::process::kill_process(pid, rustix::process::Signal::CONT) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn reap_until(child: &mut Child, deadline: Instant) -> io::Result<Option<ExitStatus>> {
    let mut next_resume = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(Some(status)),
            Ok(None) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
        let now = Instant::now();
        if now >= deadline {
            return Ok(None);
        }
        if now >= next_resume {
            // A stopped helper must finish its own descendant cleanup before
            // escalation: killing it immediately would discard adoption ownership.
            resume(child)?;
            next_resume = now + Duration::from_millis(20);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
impl Drop for Helper {
    fn drop(&mut self) {
        let Some(mut child) = self.0.take() else {
            return;
        };
        let deadline = Instant::now() + CLOSE_TIMEOUT;
        let result = reap_until(&mut child, deadline - Duration::from_millis(100));
        match result {
            Ok(Some(status)) => {
                if !status.success() {
                    eprintln!("PTY supervisor exit: {status}");
                }
                return;
            }
            Err(error) => {
                eprintln!("PTY supervisor observation: {error}");
                return;
            }
            Ok(None) => {}
        }
        eprintln!("PTY supervisor cleanup timed out; escalating to SIGKILL");
        if let Err(error) = child.kill() {
            eprintln!("PTY supervisor kill: {error}");
        }
        match reap_until(&mut child, deadline) {
            Ok(Some(_)) => return,
            Err(error) => {
                eprintln!("PTY supervisor reap: {error}");
                return;
            }
            Ok(None) => {}
        }
        // SIGKILL cannot force an uninterruptible kernel task to exit on time.
        // Retain its Child ownership on a dedicated reaper, never block the
        // terminal's Tokio owner on an unconditional wait or reuse its PID.
        if let Err(error) = std::thread::Builder::new()
            .name("pty-child-reap".into())
            .spawn(move || {
                if let Err(error) = child.wait() {
                    eprintln!("PTY delayed reap: {error}");
                }
            })
        {
            eprintln!("PTY reaper thread: {error}");
        }
    }
}
impl Drop for Supervisor {
    fn drop(&mut self) {
        // Queue close/EOF without waiting for a stopped helper's acknowledgement.
        // Helper::drop has one fixed deadline, resumes stopped children, then
        // escalates and reaps without blocking the Tokio owner indefinitely.
        let control = self.control.get_mut().get_mut();
        if let Err(error) = control.set_nonblocking(true) {
            eprintln!("PTY control flags: {error}");
        }
        let _ = control.write_all(b"X");
        let _ = control.shutdown(std::net::Shutdown::Write);
    }
}

#[cfg(test)]
#[path = "supervisor_entry_tests.rs"]
mod tests;
