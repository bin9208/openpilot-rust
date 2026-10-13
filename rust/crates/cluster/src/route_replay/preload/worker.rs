use super::{protocol, Error, ParseRequest, Request, Response};
use crate::runtime_args::CornerSource;
use openpilot_process_supervision::{CapturedChild, CapturedCommand};
use rustix::event::{poll, PollFd, PollFlags, Timespec};
use std::{
    io::{self, Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct Config {
    pub program: PathBuf,
    pub launcher: PathBuf,
    pub cwd: PathBuf,
    pub corner_source: CornerSource,
}
struct Helper(CapturedChild);
impl Drop for Helper {
    fn drop(&mut self) {
        match self.0.process.try_wait() {
            Ok(Some(_)) => (),
            Ok(None) => {
                if let Err(error) = self.0.process.kill() {
                    eprintln!("route preload kill: {error}");
                }
                if let Err(error) = self.0.process.wait() {
                    eprintln!("route preload reap: {error}");
                }
            }
            Err(error) => eprintln!("route preload wait: {error}"),
        }
    }
}
pub struct Worker {
    config: Config,
    front_radar_only: Option<bool>,
    helper: Option<Helper>,
    stream: UnixStream,
    buffer: Vec<u8>,
    _directory: tempfile::TempDir,
}
impl Worker {
    /// # Errors
    /// Returns child launch, private IPC setup or connection failures.
    pub fn new(config: Config) -> Result<Self, Error> {
        Self::configured(config, None)
    }
    pub(crate) fn configured(
        config: Config,
        front_radar_only: Option<bool>,
    ) -> Result<Self, Error> {
        let directory = tempfile::Builder::new()
            .prefix("route-preload-")
            .tempdir_in("/tmp")?;
        let path = directory.path().join("preload.sock");
        let listener = UnixListener::bind(&path)?;
        listener.set_nonblocking(true)?;
        let source = match config.corner_source {
            CornerSource::Live => "live",
            CornerSource::Raw => "raw",
            CornerSource::Stable => "stable",
        };
        let command = CapturedCommand {
            launcher: config.launcher.clone(),
            cwd: config.cwd.clone(),
            argv: vec![
                config.program.as_os_str().into(),
                "--route-preload-helper".into(),
                path.into_os_string(),
                source.into(),
            ],
        };
        let mut helper = Helper(command.spawn_inherited()?);
        let start = Instant::now();
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if helper.0.process.try_wait()?.is_some() {
                        return Err(Error::Invalid(
                            "route preload worker exited before connecting".into(),
                        ));
                    }
                    if start.elapsed() >= Duration::from_secs(5) {
                        return Err(Error::Invalid(
                            "route preload worker connection timeout".into(),
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
                Err(error) => return Err(error.into()),
            }
        };
        if let Some(front_radar_only) = front_radar_only {
            protocol::write(&mut stream, &Request::Configure { front_radar_only })?;
        }
        stream.set_nonblocking(true)?;
        Ok(Self {
            config,
            front_radar_only,
            helper: Some(helper),
            stream,
            buffer: Vec::new(),
            _directory: directory,
        })
    }
    /// # Errors
    /// Returns worker launch or IPC errors.
    pub fn request(&mut self, request: ParseRequest) -> Result<(), Error> {
        if self.helper.is_none() {
            self.restart()?;
        }
        let bytes = protocol::encode(&Request::Parse(request))?;
        let mut remaining = bytes.as_slice();
        while !remaining.is_empty() {
            match self.stream.write(remaining) {
                Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero).into()),
                Ok(count) => remaining = &remaining[count..],
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    self.wait_ready(PollFlags::OUT)?;
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
    /// # Errors
    /// Returns malformed responses, IPC errors or unexpected worker exit.
    pub fn receive(&mut self, block: bool) -> Result<Option<Response>, Error> {
        if self.helper.is_none() {
            return Ok(None);
        }
        let mut scratch = vec![0; 65_536];
        loop {
            if let Some(response) = protocol::decode(&mut self.buffer)? {
                return Ok(Some(response));
            }
            match self.stream.read(&mut scratch) {
                Ok(0) if !block => return Ok(None),
                Ok(0) => return Err(Error::Invalid("route preload worker exited".into())),
                Ok(count) => self.buffer.extend_from_slice(&scratch[..count]),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if !block {
                        return Ok(None);
                    }
                    if self.exited()? {
                        return Err(Error::Invalid("route preload worker exited".into()));
                    }
                    self.wait_ready(PollFlags::IN)?;
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
    fn wait_ready(&self, flags: PollFlags) -> Result<(), Error> {
        let timeout = Timespec {
            tv_sec: 0,
            tv_nsec: 100_000_000,
        };
        match poll(&mut [PollFd::new(&self.stream, flags)], Some(&timeout)) {
            Ok(_) | Err(rustix::io::Errno::INTR) => Ok(()),
            Err(error) => Err(io::Error::from(error).into()),
        }
    }
    fn exited(&mut self) -> Result<bool, Error> {
        match &mut self.helper {
            Some(helper) => Ok(helper.0.process.try_wait()?.is_some()),
            None => Ok(true),
        }
    }
    fn wait_for(&mut self, duration: Duration) -> Result<(), Error> {
        let start = Instant::now();
        while !self.exited()? && start.elapsed() < duration {
            std::thread::sleep(Duration::from_millis(2));
        }
        Ok(())
    }
    #[must_use]
    pub fn process_id(&self) -> Option<u32> {
        self.helper.as_ref().map(|helper| helper.0.process.id())
    }
    /// # Errors
    /// Returns child launch, signal or IPC setup errors.
    pub fn restart(&mut self) -> Result<(), Error> {
        self.close()?;
        *self = Self::configured(self.config.clone(), self.front_radar_only)?;
        Ok(())
    }
    /// # Errors
    /// Returns signal or process wait errors after stopping the owned helper.
    pub fn close(&mut self) -> Result<(), Error> {
        if self.helper.is_none() {
            return Ok(());
        }
        let result = (|| {
            if !self.exited()? {
                let _ = self.stream.write_all(&protocol::encode(&Request::Stop)?);
                self.wait_for(Duration::from_millis(500))?;
            }
            if !self.exited()? {
                let pid = self
                    .process_id()
                    .and_then(|pid| i32::try_from(pid).ok())
                    .and_then(rustix::process::Pid::from_raw)
                    .ok_or_else(|| Error::Invalid("invalid route preload PID".into()))?;
                match rustix::process::kill_process(pid, rustix::process::Signal::TERM) {
                    Ok(()) | Err(rustix::io::Errno::SRCH) => (),
                    Err(error) => return Err(io::Error::from(error).into()),
                }
                self.wait_for(Duration::from_millis(500))?;
            }
            Ok(())
        })();
        self.helper = None;
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
        self.buffer.clear();
        result
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            eprintln!("route preload close: {error}");
        }
    }
}
