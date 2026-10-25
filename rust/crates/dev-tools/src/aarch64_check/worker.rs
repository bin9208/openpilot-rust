//! Runs only inside the parent's retained owned process group. Inner children
//! inherit that group; outer cancellation kills the worker and all descendants.
use super::{check_frame, Result};
use openpilot_msgq::Subscriber;
use std::{
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
struct ChildOwner(Child);
impl Drop for ChildOwner {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
impl ChildOwner {
    fn start(binary: &Path, runner: &Path, root: &Path, args: &[&str]) -> Result<Self> {
        Ok(Self(
            Command::new(runner)
                .arg(binary)
                .arg("--proc-root")
                .arg(root)
                .args(args)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()?,
        ))
    }
    fn wait(&mut self, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.0.try_wait()? {
                return if status.success() {
                    Ok(())
                } else {
                    Err("AArch64 candidate exited unsuccessfully".into())
                };
            }
            if Instant::now() >= deadline {
                return Err("AArch64 candidate wait deadline exceeded".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
pub fn run(binary: &Path, runner: &Path, root: &Path) -> Result<()> {
    if rustix::process::getpgrp() != rustix::process::getpid() {
        return Err("private worker requires its outer owned process group".into());
    }
    ChildOwner::start(binary, runner, root, &["--self-test"])?.wait(Duration::from_secs(15))?;
    let mut subscriber = Subscriber::with_capacity(
        "procLog",
        false,
        crate::proclog_reference_check::queue_size() as usize,
    )?;
    let mut producer = ChildOwner::start(
        binary,
        runner,
        root,
        &["--publish", "--frames", "3", "--interval-ms", "100"],
    )?;
    let mut frames = 0;
    let deadline = Instant::now() + Duration::from_secs(10);
    while frames < 3 && Instant::now() < deadline {
        if let Some(bytes) = subscriber.receive(Duration::from_millis(100))? {
            check_frame(&bytes)?;
            frames += 1;
        } else if producer.0.try_wait()?.is_some() {
            break;
        }
    }
    if frames != 3 {
        return Err(format!("received {frames}/3 procLog messages").into());
    }
    producer.wait(Duration::from_secs(5))?;
    drop(producer);
    drop(subscriber);
    Ok(())
}
