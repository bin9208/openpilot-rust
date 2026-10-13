use std::{
    io::{self, Read},
    os::unix::process::CommandExt,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
struct Owned {
    child: Child,
    reaped: bool,
}
impl Owned {
    fn stop(&mut self) -> io::Result<()> {
        if self.reaped {
            return Ok(());
        }
        match rustix::process::kill_process_group(
            rustix::process::Pid::from_child(&self.child),
            rustix::process::Signal::KILL,
        ) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => {}
            Err(error) => return Err(error.into()),
        }
        self.child.wait()?;
        self.reaped = true;
        Ok(())
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.reaped {
            if let Err(error) = self.stop() {
                eprintln!("replay font inventory cleanup: {error}");
            }
        }
    }
}
fn nonblocking(pipe: &impl std::os::fd::AsFd) -> io::Result<()> {
    let flags = rustix::fs::fcntl_getfl(pipe)?;
    Ok(rustix::fs::fcntl_setfl(
        pipe,
        flags | rustix::fs::OFlags::NONBLOCK,
    )?)
}
fn drain(pipe: &mut impl Read, output: &mut Vec<u8>) -> io::Result<bool> {
    let mut buffer = [0; 8192];
    loop {
        match pipe.read(&mut buffer) {
            Ok(0) => return Ok(true),
            Ok(count) => output.extend_from_slice(&buffer[..count]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}
pub(super) fn collect() -> io::Result<Vec<u8>> {
    let start = Instant::now();
    let child = Command::new("fc-list")
        .args([":", "file", "family", "style"])
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut owned = Owned {
        child,
        reaped: false,
    };
    let mut stdout = owned
        .child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("font stdout missing"))?;
    let mut stderr = owned
        .child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("font stderr missing"))?;
    nonblocking(&stdout)?;
    nonblocking(&stderr)?;
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let mut status = None;
    loop {
        if start.elapsed() >= Duration::from_secs(1) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "font inventory timed out",
            ));
        }
        let out_done = drain(&mut stdout, &mut output)?;
        let err_done = drain(&mut stderr, &mut errors)?;
        errors.clear();
        if status.is_none() {
            status = rustix::process::waitid(
                rustix::process::WaitId::Pid(rustix::process::Pid::from_child(&owned.child)),
                rustix::process::WaitIdOptions::EXITED
                    | rustix::process::WaitIdOptions::NOHANG
                    | rustix::process::WaitIdOptions::NOWAIT,
            )?;
        }
        if let Some(status) = status {
            if out_done && err_done {
                let succeeded = status.exit_status() == Some(0);
                owned.stop()?;
                return if succeeded {
                    Ok(output)
                } else {
                    Err(io::Error::other("font inventory failed"))
                };
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
