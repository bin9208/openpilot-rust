//! Owned external commands. Keep the leader unreaped until group cleanup ends.
use crate::{config::Config, Error};
use openpilot_process_supervision::{CapturedChild, CapturedCommand};
use rustix::process::{kill_process_group, waitid, Pid, Signal, WaitId, WaitIdOptions};
use std::{
    ffi::OsString,
    io::{self, Read},
    process::{ExitStatus, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub(crate) static NEVER_CANCELLED: AtomicBool = AtomicBool::new(false);

pub(crate) fn check(cancelled: &AtomicBool) -> Result<(), Error> {
    if cancelled.load(Ordering::Relaxed) {
        Err(io::Error::new(io::ErrorKind::Interrupted, "modem command cancelled").into())
    } else {
        Ok(())
    }
}

pub(crate) struct Child {
    child: CapturedChild,
    cleanup_started: bool,
}
impl Child {
    pub(crate) fn spawn(
        config: &Config,
        argv: Vec<OsString>,
        stdout: Stdio,
        stderr: Stdio,
    ) -> Result<Self, Error> {
        let child = CapturedCommand {
            launcher: config.launcher.clone(),
            cwd: std::env::current_dir()?,
            argv,
        }
        .spawn_session_redirected_with_env(stdout, stderr, &[])?;
        Ok(Self {
            child,
            cleanup_started: false,
        })
    }

    pub(crate) fn exited(&self) -> io::Result<bool> {
        loop {
            match waitid(
                WaitId::Pid(Pid::from_child(&self.child.process)),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            ) {
                Ok(status) => return Ok(status.is_some()),
                Err(rustix::io::Errno::INTR) => continue,
                Err(error) => return Err(error.into()),
            }
        }
    }

    fn terminate(&mut self) -> io::Result<ExitStatus> {
        // No later Drop path may signal this identity after ANY reap attempt,
        // including a wait error. The leader is still retained at this point.
        self.cleanup_started = true;
        let signal_error =
            match kill_process_group(Pid::from_child(&self.child.process), Signal::KILL) {
                Ok(()) | Err(rustix::io::Errno::SRCH) => None,
                Err(error) => {
                    let _ = self.child.process.kill();
                    Some(error)
                }
            };
        let status = self.child.process.wait()?;
        match signal_error {
            Some(error) => Err(error.into()),
            None => Ok(status),
        }
    }

    pub(crate) fn wait(mut self, cancelled: &AtomicBool) -> Result<bool, Error> {
        loop {
            check(cancelled)?;
            if self.exited()? {
                return Ok(self.terminate()?.success());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        if !self.cleanup_started {
            if let Err(error) = self.terminate() {
                eprintln!("modem: command cleanup: {error}");
            }
        }
    }
}

struct Output {
    pipe: std::io::PipeReader,
    eof: bool,
}
impl Output {
    fn new(pipe: std::io::PipeReader) -> io::Result<Self> {
        let flags = rustix::fs::fcntl_getfl(&pipe)?;
        rustix::fs::fcntl_setfl(&pipe, flags | rustix::fs::OFlags::NONBLOCK)?;
        Ok(Self { pipe, eof: false })
    }
    fn drain(&mut self, output: &mut Vec<u8>) -> io::Result<bool> {
        // One bounded drain per iteration keeps a noisy helper from hiding timeout/cancel.
        let mut bytes = [0; 16 * 1024];
        match self.pipe.read(&mut bytes) {
            Ok(0) => self.eof = true,
            Ok(count) => {
                output.extend_from_slice(&bytes[..count]);
                return Ok(true);
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error),
        }
        Ok(false)
    }
}

pub(crate) fn interface(config: &Config, cancelled: &AtomicBool) -> Result<String, Error> {
    check(cancelled)?;
    let mut argv = vec![config.ip.clone().into_os_string()];
    argv.extend(
        ["-4", "addr", "show", "ppp0"]
            .into_iter()
            .map(OsString::from),
    );
    let (stdout, out) = std::io::pipe()?;
    let (stderr, err) = std::io::pipe()?;
    let mut stdout = Output::new(stdout)?;
    let mut stderr = Output::new(stderr)?;
    let mut child = Child::spawn(config, argv, out.into(), err.into())?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut output = Vec::new();
    let mut errors = Vec::new();
    loop {
        check(cancelled)?;
        if Instant::now() >= deadline {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "ip addr timeout").into());
        }
        let out_read = stdout.drain(&mut output)?;
        let err_read = stderr.drain(&mut errors)?;
        // subprocess.run(capture_output=True) waits for both pipe EOFs, not just
        // the leader. Do not kill descendants early and turn timeout into success.
        if stdout.eof && stderr.eof && child.exited()? {
            break;
        }
        if !out_read && !err_read {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    // Preserve the original ignored exit status, but surface owned cleanup errors.
    child.terminate()?;
    let text = String::from_utf8(output)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    // text=True decodes both captured streams, even though _poll_iface ignores stderr.
    std::str::from_utf8(&errors)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(text)
}
