//! Non-session direct child ownership for the Tools subprocess.run boundary.
use super::runner::{Command, Failure, Runner};
use crate::Error;
use openpilot_process_supervision::{CapturedChild, CapturedCommand};
use std::{
    os::fd::{AsFd, AsRawFd},
    process::{ExitStatus, Output},
};
use tokio::{
    io::unix::AsyncFd,
    signal::unix::{signal, Signal, SignalKind},
};

struct DirectChild {
    child: CapturedChild,
    completed: bool,
}
impl DirectChild {
    fn stop(&mut self) -> Result<(), Failure> {
        // Child::kill operates only on this still-owned, unreaped direct child.
        match self.child.process.kill() {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => {}
            Err(error) => return Err(Error::from(error).into()),
        }
        self.child.process.wait().map_err(Error::from)?;
        self.completed = true;
        Ok(())
    }
    async fn wait(&mut self, changed: &mut Signal) -> Result<ExitStatus, Failure> {
        loop {
            if let Some(status) = self.child.process.try_wait().map_err(Error::from)? {
                return Ok(status);
            }
            changed.recv().await;
        }
    }
}
impl Drop for DirectChild {
    fn drop(&mut self) {
        if !self.completed {
            if let Err(error) = self.stop() {
                eprintln!("Tools direct child fallback: {error}");
            }
        }
    }
}
fn pipe<T: AsFd + AsRawFd>(pipe: T) -> Result<AsyncFd<T>, Failure> {
    let flags = rustix::fs::fcntl_getfl(&pipe)
        .map_err(std::io::Error::from)
        .map_err(Error::from)?;
    rustix::fs::fcntl_setfl(&pipe, flags | rustix::fs::OFlags::NONBLOCK)
        .map_err(std::io::Error::from)
        .map_err(Error::from)?;
    Ok(AsyncFd::new(pipe).map_err(Error::from)?)
}
async fn read<T: AsFd + AsRawFd>(pipe: &AsyncFd<T>, output: &mut Vec<u8>) -> Result<(), Failure> {
    let mut bytes = [0; 8192];
    loop {
        let mut ready = pipe.readable().await.map_err(Error::from)?;
        match ready.try_io(|inner| {
            rustix::io::read(inner.get_ref(), &mut bytes).map_err(std::io::Error::from)
        }) {
            Ok(Ok(0)) => return Ok(()),
            Ok(Ok(count)) => output.extend_from_slice(&bytes[..count]),
            Ok(Err(error)) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Ok(Err(error)) => return Err(Error::from(error).into()),
            Err(_) => {}
        }
    }
}
pub(super) async fn run(runner: &Runner, command: Command<'_>) -> Result<Output, Failure> {
    let mut stopped = runner.stopped.clone();
    if *stopped.borrow() {
        return Err(Failure::Cancelled);
    }
    let mut changed = signal(SignalKind::child()).map_err(Error::from)?;
    let cwd = command.cwd.map_or_else(
        || std::env::current_dir().unwrap_or_else(|_| "/".into()),
        std::path::Path::to_path_buf,
    );
    let request = CapturedCommand {
        launcher: runner.launcher.clone(),
        cwd: cwd.clone(),
        argv: command.argv.iter().map(Into::into).collect(),
    };
    let child = match &runner.lock {
        Some(lock) => request.spawn_captured_with_lock(&[], lock.as_fd()),
        None => request.spawn(),
    }
    .map_err(|error| {
        super::runner::failure(error, &cwd, command.argv.first().map(String::as_str))
    })?;
    let mut child = DirectChild {
        child,
        completed: false,
    };
    let stdout = pipe(
        child
            .child
            .process
            .stdout
            .take()
            .ok_or_else(|| Error::Source("Tools stdout missing".into()))?,
    )?;
    let stderr = pipe(
        child
            .child
            .process
            .stderr
            .take()
            .ok_or_else(|| Error::Source("Tools stderr missing".into()))?,
    )?;
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let result = {
        let complete = async {
            let ((), (), status) = tokio::try_join!(
                read(&stdout, &mut out),
                read(&stderr, &mut err),
                child.wait(&mut changed)
            )?;
            Ok::<_, Failure>(status)
        };
        tokio::pin!(complete);
        tokio::select! {
            result = async {
                match command.timeout {
                    Some(timeout) => tokio::time::timeout(timeout, complete).await.map_err(|_| Failure::Timeout)?,
                    None => complete.await,
                }
            } => result,
            _ = stopped.changed() => Err(Failure::Cancelled),
        }
    };
    match result {
        Ok(status) => {
            child.completed = true;
            Ok(Output {
                status,
                stdout: out,
                stderr: err,
            })
        }
        Err(error) => {
            // Closing both pipes bounds force/timeout cleanup even if an unowned
            // descendant retained a writer; no descendant or process group is killed.
            drop(stdout);
            drop(stderr);
            child.stop()?;
            Err(error)
        }
    }
}
