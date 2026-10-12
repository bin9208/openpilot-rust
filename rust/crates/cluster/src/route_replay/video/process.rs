use std::{
    io,
    process::Child,
    time::{Duration, Instant},
};
pub(super) struct Owned {
    pub child: Child,
    reaped: bool,
}
impl Owned {
    pub const fn new(child: Child) -> Self {
        Self {
            child,
            reaped: false,
        }
    }
    fn wait_for(&mut self, duration: Duration) -> io::Result<bool> {
        let start = Instant::now();
        loop {
            if self.child.try_wait()?.is_some() {
                self.reaped = true;
                return Ok(true);
            }
            if start.elapsed() >= duration {
                return Ok(false);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    pub fn close(&mut self) -> io::Result<()> {
        self.child.stdout.take();
        if self.wait_for(Duration::ZERO)? {
            return Ok(());
        }
        match rustix::process::kill_process(
            rustix::process::Pid::from_child(&self.child),
            rustix::process::Signal::TERM,
        ) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => {}
            Err(error) => return Err(error.into()),
        }
        if self.wait_for(Duration::from_millis(300))? {
            return Ok(());
        }
        match self.child.kill() {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::InvalidInput => {}
            Err(error) => return Err(error),
        }
        if !self.wait_for(Duration::from_millis(300))? {
            // Source returns after its second timeout. Keep ownership until the
            // killed child is reaped, so an owned FFmpeg cannot become a zombie.
            self.child.wait()?;
            self.reaped = true;
        }
        Ok(())
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.reaped {
            if let Err(error) = self.child.kill() {
                eprintln!("route video kill: {error}");
            }
            if let Err(error) = self.child.wait() {
                eprintln!("route video reap: {error}");
            }
        }
    }
}
