//! One actor-owned generation; no old reader can finalize a replacement (#264).
use super::{supervisor_client::Supervisor, Config};
use crate::Error;
use openpilot_process_supervision::pty;
use std::{fs::File, sync::Arc};
use tokio::io::unix::AsyncFd;

#[cfg(test)]
#[path = "pty_tests.rs"]
mod tests;

pub(super) struct Generation {
    pub master: Arc<AsyncFd<File>>,
    supervisor: Supervisor,
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
        if command.as_bytes().contains(&0) || shell.as_encoded_bytes().contains(&0) {
            return Err(Error::Source("embedded null byte".into()));
        }
        let (supervisor, pid) = Supervisor::start(config, pair.slave, command)?;
        Ok(Self {
            master,
            supervisor,
            pid,
        })
    }
    pub fn alive(&mut self) -> Result<bool, Error> {
        Ok(self.supervisor.observe()?.is_none())
    }
    pub fn exit_code(&mut self) -> Result<Option<i32>, Error> {
        Ok(self.supervisor.observe()?)
    }
    pub fn signal(&self, signal: rustix::process::Signal) -> Result<(), Error> {
        Ok(self.supervisor.signal(signal)?)
    }
}
#[cfg(test)]
#[path = "supervisor_status.rs"]
mod status;
#[cfg(test)]
use status::exit_status;
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

#[cfg(test)]
#[path = "pty_cleanup_tests.rs"]
mod cleanup_tests;
