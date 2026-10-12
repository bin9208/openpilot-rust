use crate::Error;
use rustix::process::{kill_process, Pid, Signal};
use signal_hook::iterator::Signals;
use std::{
    fs::File,
    os::{fd::AsFd, unix::net::UnixListener},
    process::{Child, Command, ExitStatus},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

struct OwnedChild(Child);
impl OwnedChild {
    fn signal(&self, signal: Signal) -> Result<(), Error> {
        let pid = i32::try_from(self.0.id())
            .ok()
            .and_then(Pid::from_raw)
            .ok_or_else(|| Error::Configuration("startup child PID out of range".into()))?;
        match kill_process(pid, signal) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
            Err(error) => Err(std::io::Error::from(error).into()),
        }
    }
    fn forward(
        &self,
        stopped: &AtomicBool,
        signals: &mut Option<&mut Signals>,
        sent: &mut bool,
    ) -> Result<(), Error> {
        if let Some(signals) = signals {
            for signal in signals.pending() {
                self.signal(match signal {
                    signal_hook::consts::SIGINT => Signal::INT,
                    signal_hook::consts::SIGTERM => Signal::TERM,
                    _ => return Err(Error::Configuration("unexpected startup signal".into())),
                })?;
                *sent = true;
            }
        } else if stopped.load(Ordering::Relaxed) && !*sent {
            self.signal(Signal::TERM)?;
            *sent = true;
        }
        Ok(())
    }
    fn finish(
        mut self,
        stopped: &AtomicBool,
        signals: &mut Option<&mut Signals>,
        mut sent: bool,
    ) -> Result<ExitStatus, Error> {
        loop {
            self.forward(stopped, signals, &mut sent)?;
            match self.0.try_wait() {
                Ok(Some(status)) => return Ok(status),
                Ok(None) => thread::sleep(Duration::from_millis(25)),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(Some(_))) {
            return;
        }
        if let Err(error) = self.signal(Signal::TERM) {
            eprintln!("startup child termination: {error}");
        }
        loop {
            match self.0.wait() {
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => eprintln!("startup child reap: {error}"),
                Ok(_) => {}
            }
            break;
        }
    }
}

pub fn run_locked(
    command: Command,
    lock: &File,
    stopped: &AtomicBool,
) -> Result<ExitStatus, Error> {
    run(command, lock, stopped, None)
}

pub fn run_locked_with_signals(
    command: Command,
    lock: &File,
    stopped: &AtomicBool,
    signals: &mut Signals,
) -> Result<ExitStatus, Error> {
    run(command, lock, stopped, Some(signals))
}

fn run(
    mut command: Command,
    lock: &File,
    stopped: &AtomicBool,
    mut signals: Option<&mut Signals>,
) -> Result<ExitStatus, Error> {
    let directory = tempfile::Builder::new()
        .prefix("op-boot-")
        .tempdir_in("/tmp")?;
    let path = directory.path().join("lock.sock");
    let listener = UnixListener::bind(&path)?;
    listener.set_nonblocking(true)?;
    command
        .env("CARROT_BOOT_LOCK_SOCKET", &path)
        .env_remove("CARROT_BOOT_LOCK_FD");
    let mut child = OwnedChild(command.spawn()?);
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut sent = false;
    loop {
        child.forward(stopped, &mut signals, &mut sent)?;
        if stopped.load(Ordering::Relaxed) || sent {
            return child.finish(stopped, &mut signals, sent);
        }
        if let Some(status) = child.0.try_wait()? {
            return Err(Error::Configuration(format!(
                "startup child exited before taking repository lock: {status}"
            )));
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                stream.set_write_timeout(Some(Duration::from_secs(2)))?;
                openpilot_process_supervision::send_boot_lock(&mut stream, lock.as_fd())?;
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error.into()),
        }
        if Instant::now() >= deadline {
            return Err(Error::Configuration(
                "startup child did not request repository lock within 60 seconds".into(),
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }
    drop(listener);
    drop(directory);
    child.finish(stopped, &mut signals, sent)
}
