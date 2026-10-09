use super::{
    jobs::Store,
    runner::{code, Command, Completed, Failure, Runner},
};
use crate::{Error, Value};
use openpilot_process_supervision::{CapturedChild, CapturedCommand};
use std::{io::Read, os::fd::AsFd, process::ExitStatus, sync::Arc, time::Duration};
use tokio::{
    io::unix::AsyncFd,
    signal::unix::{signal, Signal, SignalKind},
};

struct Group {
    child: CapturedChild,
    pid: rustix::process::Pid,
    completed: bool,
    observed: bool,
}
impl Group {
    fn send(&self, signal: rustix::process::Signal) -> Result<(), Failure> {
        match rustix::process::kill_process_group(self.pid, signal) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
            Err(error) => Err(Error::from(std::io::Error::from(error)).into()),
        }
    }
    async fn wait(&mut self, changed: &mut Signal) -> Result<ExitStatus, Failure> {
        loop {
            if let Some(status) = self.child.process.try_wait().map_err(Error::from)? {
                self.observed = true;
                return Ok(status);
            }
            changed.recv().await;
        }
    }
    async fn stop(
        &mut self,
        changed: &mut Signal,
        pipe: &AsyncFd<std::io::PipeReader>,
        output: &mut Vec<u8>,
        log: &Option<(Arc<Store>, String)>,
    ) -> Result<(), Failure> {
        self.send(rustix::process::Signal::TERM)?;
        let observed = self.observed;
        let grace = async {
            if observed {
                self.wait(changed).await?;
            } else {
                tokio::try_join!(self.wait(changed), read(pipe, output, log))?;
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
                eprintln!("Tools group fallback: {error}");
            }
            if let Err(error) = self.child.process.wait() {
                eprintln!("Tools child reap: {error}");
            }
        }
    }
}
async fn read(
    pipe: &AsyncFd<std::io::PipeReader>,
    output: &mut Vec<u8>,
    log: &Option<(Arc<Store>, String)>,
) -> Result<(), Failure> {
    let mut buffer = [0; 1024];
    loop {
        let mut ready = pipe.readable().await.map_err(Error::from)?;
        match ready.try_io(|inner| {
            let mut pipe = inner.get_ref();
            pipe.read(&mut buffer)
        }) {
            Ok(Ok(0)) => return Ok(()),
            Ok(Ok(count)) => {
                if let Some((store, id)) = log {
                    store.append(id, &Value::text(&String::from_utf8_lossy(&buffer[..count])))?;
                } else {
                    output.extend_from_slice(&buffer[..count]);
                }
            }
            Ok(Err(error)) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Ok(Err(error)) => return Err(Error::from(error).into()),
            Err(_) => {}
        }
    }
}
pub(super) async fn run(
    runner: &Runner,
    command: Command<'_>,
    log: Option<(Arc<Store>, String)>,
) -> Result<Completed, Failure> {
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
    let captured = match &runner.lock {
        Some(lock) => request.spawn_session_merged_with_lock(&[], lock.as_fd()),
        None => request.spawn_session_merged_with_env(&[]),
    };
    let (child, pipe) = captured.map_err(|error| {
        super::runner::failure(error, &cwd, command.argv.first().map(String::as_str))
    })?;
    let pid = i32::try_from(child.process.id())
        .ok()
        .and_then(rustix::process::Pid::from_raw)
        .ok_or_else(|| Error::Source("Tools PID out of range".into()))?;
    let mut group = Group {
        child,
        pid,
        completed: false,
        observed: false,
    };
    let flags = rustix::fs::fcntl_getfl(&pipe)
        .map_err(std::io::Error::from)
        .map_err(Error::from)?;
    rustix::fs::fcntl_setfl(&pipe, flags | rustix::fs::OFlags::NONBLOCK)
        .map_err(std::io::Error::from)
        .map_err(Error::from)?;
    let pipe = AsyncFd::new(pipe).map_err(Error::from)?;
    let mut output = Vec::new();
    let complete = async {
        let ((), status) =
            tokio::try_join!(read(&pipe, &mut output, &log), group.wait(&mut changed))?;
        Ok::<_, Failure>(status)
    };
    let completed = {
        tokio::pin!(complete);
        tokio::select! {
            result=async { match command.timeout {Some(timeout)=>tokio::time::timeout(timeout,complete).await.map_err(|_|Failure::Timeout)?,None=>complete.await} }=>Some(result),
            _=stopped.changed()=>None,
        }
    };
    match completed {
        Some(Ok(status)) => {
            group.completed = true;
            Ok(Completed {
                streams: None,
                code: code(status),
                output: String::from_utf8_lossy(&output)
                    .trim_matches(super::text::whitespace)
                    .into(),
            })
        }
        result => {
            group.stop(&mut changed, &pipe, &mut output, &log).await?;
            read(&pipe, &mut output, &log).await?;
            match result {
                Some(Err(error)) => {
                    if matches!(error, Failure::Timeout) {
                        if let Some((store, id)) = log {
                            store.append(&id, &Value::text("\n[timeout]\n"))?;
                        }
                    }
                    Err(error)
                }
                None => Err(Failure::Cancelled),
                Some(Ok(_)) => Err(Error::Source("unexpected Tools completion".into()).into()),
            }
        }
    }
}
