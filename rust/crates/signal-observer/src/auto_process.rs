//! Native process boundary for the automatic owner. It launches no model itself.
use crate::auto::{Error, Owner};
use std::{
    process::{Child, Command, ExitStatus},
    thread,
    time::{Duration, Instant},
};

pub struct ProcessOwner {
    child: Child,
    status: Option<ExitStatus>,
}
impl ProcessOwner {
    pub fn spawn(command: &mut Command) -> Result<Self, Error> {
        Ok(Self {
            child: command.spawn()?,
            status: None,
        })
    }
    fn status(&mut self) -> Result<Option<ExitStatus>, Error> {
        if self.status.is_none() {
            self.status = self.child.try_wait()?;
        }
        Ok(self.status)
    }
}
impl Owner for ProcessOwner {
    fn pid(&self) -> u32 {
        self.child.id()
    }
    fn is_alive(&mut self) -> Result<bool, Error> {
        Ok(self.status()?.is_none())
    }
    fn terminate(&mut self) -> Result<(), Error> {
        let pid = rustix::process::Pid::from_raw(i32::try_from(self.child.id())?)
            .ok_or("invalid signal backend PID")?;
        match rustix::process::kill_process(pid, rustix::process::Signal::TERM) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
    fn join(&mut self, timeout: Duration) -> Result<(), Error> {
        let deadline = Instant::now() + timeout;
        while self.status()?.is_none() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }
    fn kill(&mut self) -> Result<(), Error> {
        match self.child.kill() {
            Ok(()) => Ok(()),
            Err(error) if error.raw_os_error() == Some(rustix::io::Errno::SRCH.raw_os_error()) => {
                Ok(())
            }
            Err(error) => Err(error.into()),
        }
    }
    fn exit_code(&mut self) -> Result<Option<i32>, Error> {
        use std::os::unix::process::ExitStatusExt;
        Ok(self.status()?.and_then(|status| {
            status
                .code()
                .or_else(|| status.signal().map(|signal| -signal))
        }))
    }
    fn close(self) {
        drop(self);
    }
}
