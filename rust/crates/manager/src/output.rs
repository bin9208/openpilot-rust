use crate::Error;
use openpilot_process_supervision::{pty::Pair, CapturedChild, CapturedCommand};
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
    process::{kill_process, Pid, Signal},
};
use std::{io::Read, process::ExitStatus, time::Duration};

struct OwnedChild(CapturedChild);
impl OwnedChild {
    fn signal(&self, signal: Signal) -> Result<(), Error> {
        let pid = i32::try_from(self.0.process.id())
            .ok()
            .and_then(Pid::from_raw)
            .ok_or(Error::Contract("stdout child PID out of range"))?;
        match kill_process(pid, signal) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
            Err(error) => Err(std::io::Error::from(error).into()),
        }
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if matches!(self.0.process.try_wait(), Ok(Some(_))) {
            return;
        }
        if let Err(error) = self.signal(Signal::TERM) {
            eprintln!("stdout child termination: {error}");
        }
        loop {
            match self.0.process.wait() {
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => eprintln!("stdout child reap: {error}"),
                Ok(_) => {}
            }
            break;
        }
    }
}

pub fn run(request: &CapturedCommand) -> Result<ExitStatus, Error> {
    let mut signals = signal_hook::iterator::Signals::new([
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
    ])?;
    let stdout = rustix::stdio::stdout();
    let flags = fcntl_getfl(stdout).map_err(std::io::Error::from)?;
    fcntl_setfl(stdout, flags | OFlags::NONBLOCK).map_err(std::io::Error::from)?;
    let Pair { mut master, slave } = Pair::open(0, 0)?;
    let mut child = OwnedChild(request.spawn_controlling_pty(slave)?);
    let timeout = Timespec::try_from(Duration::from_millis(25)).map_err(std::io::Error::other)?;
    let mut packet = [0; 4096];
    let mut open = true;
    loop {
        for signal in signals.pending() {
            child.signal(match signal {
                signal_hook::consts::SIGINT => Signal::INT,
                signal_hook::consts::SIGTERM => Signal::TERM,
                _ => return Err(Error::Contract("unexpected stdout forwarding signal")),
            })?;
        }
        if !open {
            match child.0.process.try_wait()? {
                Some(status) => return Ok(status),
                None => std::thread::sleep(Duration::from_millis(25)),
            }
            continue;
        }
        let mut descriptors = [PollFd::new(&master, PollFlags::IN)];
        match poll(&mut descriptors, Some(&timeout)) {
            Ok(0) | Err(rustix::io::Errno::INTR) => continue,
            Ok(_) => {}
            Err(error) => return Err(std::io::Error::from(error).into()),
        }
        match master.read(&mut packet) {
            Ok(0) => open = false,
            Ok(length) => {
                if std::str::from_utf8(&packet[..length]).is_ok() {
                    // Source drops an undecodable or unwritable complete packet.
                    rustix::io::write(stdout, &packet[..length]).unwrap_or(0);
                }
            }
            Err(error) if error.raw_os_error() == Some(rustix::io::Errno::IO.raw_os_error()) => {
                open = false
            }
            Err(_) => continue,
        }
    }
}
