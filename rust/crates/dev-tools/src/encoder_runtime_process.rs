//! Owned child shutdown and original nested-finally cleanup policy.
use crate::seal::Result;
use std::{
    fs,
    os::unix::process::ExitStatusExt,
    path::Path,
    process::{Child, ExitStatus},
    time::{Duration, Instant},
};
fn code(status: ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| -status.signal().unwrap_or(1))
}
fn wait(child: &mut Child, timeout: Duration) -> Result<Option<ExitStatus>> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if start.elapsed() >= timeout {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
pub fn stop_cleanly(child: &mut Child, receipt: &Path, timeout: Duration) -> Result<()> {
    let signaled = child.try_wait()?.is_none();
    if signaled {
        let pid = rustix::process::Pid::from_raw(i32::try_from(child.id())?)
            .ok_or("invalid owned child pid")?;
        match rustix::process::kill_process(pid, rustix::process::Signal::TERM) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => {}
            Err(e) => return Err(e.into()),
        }
    }
    let (status, timed_out) = match wait(child, timeout)? {
        Some(s) => (s, false),
        None => {
            child.kill()?;
            (
                wait(child, Duration::from_secs(5))?.ok_or("owned encoder kill deadline")?,
                true,
            )
        }
    };
    let code = code(status);
    fs::write(
        receipt,
        format!(
            "{{\"pid\": {}, \"signaled\": {}, \"timed_out\": {}, \"returncode\": {code}}}\n",
            child.id(),
            if signaled { "true" } else { "false" },
            if timed_out { "true" } else { "false" }
        ),
    )?;
    if timed_out {
        return Err(format!("encoder shutdown timed out: {}", receipt.display()).into());
    }
    if code != 0 {
        return Err(format!(
            "encoder shutdown failed with status {code}: {}",
            receipt.display()
        )
        .into());
    }
    Ok(())
}
pub trait Cleanup {
    fn shutdown(&mut self) -> Result<()>;
    fn drain(&mut self) -> Result<()>;
    fn close_files(&mut self) -> Result<()>;
    fn write_packets(&mut self) -> Result<()>;
    fn write_inputs(&mut self) -> Result<()>;
    fn release_ipc(&mut self) -> Result<()>;
    fn outcome(&mut self) -> Result<()>;
}
pub fn close(peer: &mut dyn Cleanup) -> Result<()> {
    let stopped = peer.shutdown();
    let drained = peer.drain();
    peer.close_files()?;
    peer.write_packets()?;
    peer.write_inputs()?;
    peer.release_ipc()?;
    drained?;
    stopped?;
    peer.outcome()
}
