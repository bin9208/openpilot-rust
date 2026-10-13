use super::Config;
use crate::model_delivery::{failure::Kind, Error};
use rustix::process::{
    kill_process_group, waitid, Pid, Signal, WaitId, WaitIdOptions, WaitIdStatus,
};
use std::{
    io::{Read, Seek, Write},
    os::unix::process::CommandExt,
    path::Path,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

#[cfg(test)]
#[path = "runner_tests.rs"]
mod tests;

pub(super) struct Failed {
    pub detail: String,
    pub kind: Kind,
}
impl From<Error> for Failed {
    fn from(error: Error) -> Self {
        let kind = match &error {
            Error::Io(error) | Error::Native(crate::Error::Io(error)) => match error.kind() {
                std::io::ErrorKind::TimedOut => Kind::Timeout,
                std::io::ErrorKind::BrokenPipe => Kind::BrokenPipe,
                _ => Kind::Detail,
            },
            Error::Native(_) | Error::Json(_) | Error::Http(_) | Error::Invalid(_) => Kind::Detail,
        };
        Self {
            detail: error.to_string(),
            kind,
        }
    }
}

struct OwnedChild {
    child: Child,
    stopped: bool,
}
impl OwnedChild {
    fn pid(&self) -> std::io::Result<Pid> {
        i32::try_from(self.child.id())
            .ok()
            .and_then(Pid::from_raw)
            .ok_or_else(|| std::io::Error::other("invalid owned runner PID"))
    }
    fn observe(&self) -> std::io::Result<Option<WaitIdStatus>> {
        loop {
            match waitid(
                WaitId::Pid(self.pid()?),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            ) {
                Err(rustix::io::Errno::INTR) => continue,
                result => return result.map_err(std::io::Error::from),
            }
        }
    }
    fn signal(&self, signal: Signal) -> std::io::Result<()> {
        match kill_process_group(self.pid()?, signal) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
    fn stop(&mut self) -> std::io::Result<()> {
        if self.stopped {
            return Ok(());
        }
        self.signal(Signal::TERM)?;
        let started = Instant::now();
        while self.observe()?.is_none() && started.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(5));
        }
        // Keep the leader unreaped until the last group signal reserves its numeric PGID.
        self.signal(Signal::KILL)?;
        self.child.wait()?;
        self.stopped = true;
        Ok(())
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            eprintln!("model boot cleanup/reap: {error}");
        }
    }
}

fn wait(child: &mut OwnedChild, cancelled: &AtomicBool, limit: Duration) -> Result<bool, Error> {
    let started = Instant::now();
    loop {
        if cancelled.load(Ordering::Relaxed) {
            child.stop()?;
            return Err(crate::Error::Cancelled.into());
        }
        if let Some(exit) = child.observe()? {
            child.stop()?;
            return Ok(exit.exit_status() == Some(0));
        }
        if started.elapsed() >= limit {
            child.stop()?;
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "precompiled boot validation timed out",
            )
            .into());
        }
        thread::sleep(Duration::from_millis(5));
    }
}

pub(super) fn validate(
    config: &Config,
    model: &Path,
    cameras: &[[u32; 2]],
    binding: &crate::worker_artifact::Binding,
    cancelled: &AtomicBool,
) -> Result<Option<Failed>, Error> {
    let mut log = tempfile::tempfile()?;
    let mut command = Command::new(&config.runner);
    command
        .process_group(0)
        .arg("--smoke")
        .arg(model)
        .arg("--worker")
        .arg(&config.worker)
        .arg("--assets")
        .arg(binding.root())
        .arg("--expect-assets-manifest")
        .arg(binding.manifest_sha256());
    for camera in cameras {
        command
            .arg("--camera")
            .arg(format!("{}x{}", camera[0], camera[1]));
    }
    let mut child = OwnedChild {
        child: command
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log.try_clone()?)
            .spawn()?,
        stopped: false,
    };
    let result = wait(&mut child, cancelled, Duration::from_secs(300));
    if matches!(result, Err(Error::Native(crate::Error::Cancelled))) {
        return result.map(|_| None);
    }
    log.rewind()?;
    let mut bytes = Vec::new();
    log.read_to_end(&mut bytes)?;
    let detail = match &result {
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::TimedOut => {
            String::from_utf8_lossy(&bytes).into_owned()
        }
        _ => String::from_utf8(bytes).map_err(crate::Error::from)?,
    };
    std::io::stdout().lock().write_all(detail.as_bytes())?;
    match result {
        Ok(true) => Ok(None),
        Ok(false) => Ok(Some(Failed {
            detail,
            kind: Kind::Detail,
        })),
        Err(error) => {
            let mut failed = Failed::from(error);
            if !detail.is_empty() {
                failed.detail = detail;
            }
            Ok(Some(failed))
        }
    }
}
