//! One actor-owned generation; no old reader can finalize a replacement (#264).
use super::Config;
use crate::Error;
use openpilot_process_supervision::{pty, CapturedChild, CapturedCommand};
use std::{fs::File, sync::Arc};
use tokio::io::unix::AsyncFd;

#[cfg(test)]
#[path = "pty_tests.rs"]
mod tests;

pub(super) struct Generation {
    pub master: Arc<AsyncFd<File>>,
    child: CapturedChild,
    pub pid: rustix::process::Pid,
}
impl Generation {
    pub fn start(config: &Config, command: &str) -> Result<Self, Error> {
        let pair = pty::Pair::open(30, 100)?;
        let flags = rustix::fs::fcntl_getfl(&pair.master).map_err(std::io::Error::from)?;
        rustix::fs::fcntl_setfl(&pair.master, flags | rustix::fs::OFlags::NONBLOCK)
            .map_err(std::io::Error::from)?;
        let master = Arc::new(AsyncFd::new(pair.master)?);
        let shell = Config::shell();
        let child = CapturedCommand {
            launcher: config.launcher.clone(),
            cwd: std::env::current_dir()?,
            argv: vec![shell.clone(), "-lc".into(), command.into()],
        }
        .spawn_session_pty_with_env(pair.slave, &Config::shell_environment())
        .map_err(|error| match error {
            openpilot_process_supervision::Error::Io(error) => {
                crate::state::io_error(error, std::path::Path::new(&shell))
            }
            openpilot_process_supervision::Error::Nul(_) => {
                Error::Source("embedded null byte".into())
            }
            error => Error::Source(error.to_string()),
        })?;
        let pid = i32::try_from(child.process.id())
            .ok()
            .and_then(rustix::process::Pid::from_raw)
            .ok_or_else(|| Error::Source("PTY process ID exceeds OS range".into()))?;
        Ok(Self { master, child, pid })
    }
    pub fn alive(&mut self) -> Result<bool, Error> {
        Ok(self.observe()?.is_none())
    }
    pub fn exit_code(&mut self) -> Result<Option<i32>, Error> {
        Ok(self.observe()?.map(exit_status))
    }
    fn observe(&self) -> Result<Option<rustix::process::WaitIdStatus>, Error> {
        use rustix::process::{waitid, WaitId, WaitIdOptions};
        // Reserve the unreaped leader identity until owned-group signals finish.
        loop {
            match waitid(
                WaitId::Pid(self.pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            ) {
                Ok(status) => return Ok(status),
                Err(rustix::io::Errno::INTR) => {}
                Err(error) => return Err(std::io::Error::from(error).into()),
            }
        }
    }
    pub fn signal(&self, signal: rustix::process::Signal) -> Result<(), Error> {
        match rustix::process::kill_process_group(self.pid, signal) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
            Err(error) => Err(std::io::Error::from(error).into()),
        }
    }
}
fn exit_status(status: rustix::process::WaitIdStatus) -> i32 {
    status
        .exit_status()
        .unwrap_or_else(|| -status.terminating_signal().unwrap_or(0))
}
impl Drop for Generation {
    fn drop(&mut self) {
        // Explicit native final ownership: source leaves a blocking threadpool
        // read alive at runtime exit. No additional graceful window is added.
        for signal in [rustix::process::Signal::HUP, rustix::process::Signal::KILL] {
            if let Err(error) = self.signal(signal) {
                eprintln!("PTY owned group close: {error}");
            }
        }
        if let Err(error) = self.child.process.wait() {
            eprintln!("PTY child reap: {error}");
        }
    }
}
pub(super) async fn read(fd: Arc<AsyncFd<File>>) -> Result<Vec<u8>, Error> {
    let mut bytes = [0; 4096];
    loop {
        let mut ready = fd.readable().await?;
        match ready
            .try_io(|fd| rustix::io::read(fd.get_ref(), &mut bytes).map_err(std::io::Error::from))
        {
            Ok(Ok(count)) => return Ok(bytes[..count].to_vec()),
            Ok(Err(error))
                if error.raw_os_error() == Some(rustix::io::Errno::IO.raw_os_error()) =>
            {
                return Ok(Vec::new())
            }
            Ok(Err(error)) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Ok(Err(error)) => return Err(error.into()),
            Err(_) => {}
        }
    }
}
pub(super) async fn write(fd: Arc<AsyncFd<File>>, mut bytes: &[u8]) -> Result<(), Error> {
    while !bytes.is_empty() {
        let mut ready = fd.writable().await?;
        match ready
            .try_io(|fd| rustix::io::write(fd.get_ref(), bytes).map_err(std::io::Error::from))
        {
            Ok(Ok(0)) => return Err(Error::Source("pty write failed".into())),
            Ok(Ok(count)) => bytes = &bytes[count..],
            Ok(Err(error)) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Ok(Err(error)) => return Err(error.into()),
            Err(_) => {}
        }
    }
    Ok(())
}
