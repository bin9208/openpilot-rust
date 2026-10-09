use super::{read::Context, Failure};
use openpilot_process_supervision::{CapturedChild, CapturedCommand};
use std::{
    io::Read,
    os::{fd::AsFd, unix::process::ExitStatusExt},
    process::ExitStatus,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::unix::AsyncFd,
    signal::unix::{signal, Signal, SignalKind},
};

struct Group {
    child: CapturedChild,
    pid: rustix::process::Pid,
    completed: bool,
    leader_observed: bool,
}

impl Group {
    fn send(&self, signal: rustix::process::Signal) -> Result<(), Failure> {
        match rustix::process::kill_process_group(self.pid, signal) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
            Err(error) => Err(std::io::Error::from(error).into()),
        }
    }

    async fn wait(&mut self, changed: &mut Signal) -> Result<ExitStatus, Failure> {
        loop {
            if let Some(status) = self.child.process.try_wait()? {
                self.leader_observed = true;
                return Ok(status);
            }
            changed.recv().await;
        }
    }

    async fn stop(
        &mut self,
        changed: &mut Signal,
        pipe: &AsyncFd<std::io::PipeReader>,
        bytes: &mut Vec<u8>,
    ) -> Result<(), Failure> {
        let leader_observed = self.leader_observed;
        self.send(rustix::process::Signal::TERM)?;
        let grace = async {
            if leader_observed {
                self.wait(changed).await?;
            } else {
                tokio::try_join!(self.wait(changed), read(pipe, bytes))?;
            }
            Ok::<_, Failure>(())
        };
        if let Ok(result) = tokio::time::timeout(Duration::from_secs(1), grace).await {
            result?;
        }
        self.send(rustix::process::Signal::KILL)?;
        self.wait(changed).await?;
        self.completed = true;
        Ok(())
    }
}

impl Drop for Group {
    fn drop(&mut self) {
        if !self.completed {
            if let Err(error) = self.send(rustix::process::Signal::KILL) {
                eprintln!("Git status command cleanup: {error}");
            }
            if let Err(error) = self.child.process.wait() {
                eprintln!("Git status command reap: {error}");
            }
        }
    }
}

async fn read(pipe: &AsyncFd<std::io::PipeReader>, bytes: &mut Vec<u8>) -> Result<(), Failure> {
    let mut buffer = [0; 16384];
    loop {
        let mut ready = pipe.readable().await?;
        match ready.try_io(|inner| {
            let mut pipe = inner.get_ref();
            pipe.read(&mut buffer)
        }) {
            Ok(Ok(0)) => return Ok(()),
            Ok(Ok(count)) => bytes.extend_from_slice(&buffer[..count]),
            Ok(Err(error)) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Ok(Err(error)) => return Err(error.into()),
            Err(_) => {}
        }
    }
}

pub(super) async fn run(
    context: &Context<'_>,
    args: &[&str],
    timeout: Duration,
) -> Result<(i32, String), Failure> {
    let mut stopped = context.stopped.clone();
    if *stopped.borrow() {
        return Err(Failure::Cancelled);
    }
    let mut changed = signal(SignalKind::child())?;
    let request = CapturedCommand {
        launcher: context.service.repository.launcher.clone(),
        cwd: context.service.repository.directory.clone(),
        argv: std::iter::once("git")
            .chain(args.iter().copied())
            .map(Into::into)
            .collect(),
    };
    let lock = Arc::clone(&context.lock);
    let (child, pipe) = tokio::task::spawn_blocking(move || {
        request.spawn_session_merged_with_lock(&[], lock.as_fd())
    })
    .await??;
    let pid = i32::try_from(child.process.id())
        .ok()
        .and_then(rustix::process::Pid::from_raw)
        .ok_or_else(|| std::io::Error::other("Git process ID out of range"))?;
    let mut group = Group {
        child,
        pid,
        completed: false,
        leader_observed: false,
    };
    let flags = rustix::fs::fcntl_getfl(&pipe).map_err(std::io::Error::from)?;
    rustix::fs::fcntl_setfl(&pipe, flags | rustix::fs::OFlags::NONBLOCK)
        .map_err(std::io::Error::from)?;
    let pipe = AsyncFd::new(pipe)?;
    let mut bytes = Vec::new();
    let completed = tokio::select! {
        result = tokio::time::timeout(timeout, async {
            let ((), status) = tokio::try_join!(read(&pipe, &mut bytes), group.wait(&mut changed))?;
            Ok::<_, Failure>(status)
        }) => Some(result),
        _ = stopped.changed() => None,
    };
    match completed {
        None => {
            group.stop(&mut changed, &pipe, &mut bytes).await?;
            read(&pipe, &mut bytes).await?;
            Err(Failure::Cancelled)
        }
        Some(Err(_)) => {
            group.stop(&mut changed, &pipe, &mut bytes).await?;
            read(&pipe, &mut bytes).await?;
            Ok((124, "timeout".into()))
        }
        Some(Ok(result)) => {
            let status = result?;
            group.completed = true;
            let code = status
                .code()
                .unwrap_or_else(|| -status.signal().unwrap_or(1));
            let output = String::from_utf8_lossy(&bytes)
                .trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
                .to_owned();
            Ok((code, output))
        }
    }
}
