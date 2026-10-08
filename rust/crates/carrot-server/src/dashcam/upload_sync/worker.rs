use super::{owner::Config, state, SyncState};
use crate::{Error, Value};
use openpilot_dashcam_upload::worker::{Event, Packet, Request};
use std::{os::unix::process::CommandExt, process::Stdio, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::watch,
};

struct Group {
    pid: rustix::process::Pid,
    completed: bool,
}
impl Group {
    fn kill(&self) -> Result<(), std::io::Error> {
        match rustix::process::kill_process_group(self.pid, rustix::process::Signal::KILL) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}
impl Drop for Group {
    fn drop(&mut self) {
        if !self.completed {
            if let Err(error) = self.kill() {
                eprintln!("Dashcam sync process group cleanup: {error}");
            }
        }
    }
}
async fn packets(child: &mut Child, request: Request) -> Result<Value, Error> {
    let mut input = child
        .stdin
        .take()
        .ok_or_else(|| Error::Source("sync worker control pipe missing".into()))?;
    let output = child
        .stdout
        .take()
        .ok_or_else(|| Error::Source("sync worker output pipe missing".into()))?;
    let mut bytes =
        serde_json::to_vec(&request).map_err(|error| Error::Source(error.to_string()))?;
    bytes.push(b'\n');
    input.write_all(&bytes).await?;
    input.flush().await?;
    let mut lines = BufReader::new(output).lines();
    let mut finish = None;
    while let Some(line) = lines.next_line().await? {
        let packet: Packet =
            serde_json::from_str(&line).map_err(|error| Error::Source(error.to_string()))?;
        match packet.event {
            Event::Finish { patch } => finish = Some(patch),
            Event::Touch
            | Event::Append { .. }
            | Event::Progress { .. }
            | Event::Context { .. }
            | Event::Partial { .. } => {}
        }
    }
    let finish =
        finish.ok_or_else(|| Error::Source("sync worker ended without a final result".into()))?;
    if let Some(error) = finish.error {
        return Err(Error::Source(error));
    }
    let result = finish
        .result
        .ok_or_else(|| Error::Source("sync worker final result missing".into()))?;
    let json = serde_json::to_string(&result).map_err(|error| Error::Source(error.to_string()))?;
    Value::parse(&json).map_err(Error::from)
}
pub(super) async fn run(
    config: Arc<Config>,
    segments: Vec<String>,
    mut stopped: watch::Receiver<SyncState>,
) -> Result<Value, Error> {
    if stopped.borrow().phase == state::Phase::Force {
        return Err(Error::Source("dashcam sync upload service stopped".into()));
    }
    let mut command = Command::new(&config.executable);
    command
        .arg("--worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    command.as_std_mut().process_group(0);
    let mut child = command.spawn()?;
    let pid = child
        .id()
        .and_then(|pid| i32::try_from(pid).ok())
        .and_then(rustix::process::Pid::from_raw)
        .ok_or_else(|| Error::Source("sync worker process ID unavailable".into()))?;
    let mut group = Group {
        pid,
        completed: false,
    };
    let request = Request {
        parent_pid: std::process::id(),
        root: config.root.clone(),
        id: uuid::Uuid::new_v4().simple().to_string(),
        segments,
        settings: config.settings.clone(),
    };
    let result = tokio::select! {
        result = packets(&mut child, request) => result,
        _ = state::forced(&mut stopped) => Err(Error::Source("dashcam sync upload service stopped".into())),
    };
    let termination = if result.is_err() {
        group.kill()
    } else {
        Ok(())
    };
    let status = tokio::select! {
        status = child.wait() => status,
        _ = state::forced(&mut stopped) => {
            let killed = group.kill();
            let status = child.wait().await;
            killed?;
            status
        }
    };
    group.completed = true;
    termination?;
    let status = status?;
    if !status.success() && result.is_ok() {
        return Err(Error::Source(format!("sync worker exited with {status}")));
    }
    result
}
