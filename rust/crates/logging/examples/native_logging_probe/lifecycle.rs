//! Opt-in cancellation fixture in the existing probe, never a logging oracle.
//! No executable, argv or reference path is read from the inert context.
use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn record(root: &Path, role: &str) -> Result<()> {
    let pending = root.join(format!("{role}.pending"));
    let row = serde_json::json!({
        "role":role,"pid":std::process::id(),
        "parent":rustix::process::getppid().ok_or("fixture parent missing")?.as_raw_pid(),
        "group":rustix::process::getpgrp().as_raw_pid(),
        "session":rustix::process::getsid(None)?.as_raw_pid()
    });
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)?;
    file.write_all(&serde_json::to_vec(&row)?)?;
    drop(file);
    fs::rename(pending, root.join(format!("{role}.json")))?;
    Ok(())
}
pub(super) struct ChildGuard(std::process::Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        // Normal/error-return fallback only. Signal-killed real peers cannot run
        // Drop; adoption and reaping then belong to the actual checker.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn spawn(role: &str) -> Result<ChildGuard> {
    // Fixed self-exec only, bound externally to the same actual fresh build.
    Ok(ChildGuard(
        Command::new(std::env::current_exe()?)
            .arg(role)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
    ))
}
pub(super) fn run() -> Result<(bool, Option<ChildGuard>)> {
    let Some(enabled) = std::env::var_os("NATIVE_LOGGING_QA_ESCAPED") else {
        return Ok((false, None));
    };
    if enabled != "1" || std::env::var("NATIVE_LOGGING_QA_HOLD").as_deref() != Ok("first") {
        return Err("native logging escaped fixture requires exact first-held opt-in".into());
    }
    let records = PathBuf::from(
        std::env::var_os("NATIVE_LOGGING_QA_RECORDS").ok_or("fixture records missing")?,
    );
    let metadata = fs::symlink_metadata(&records)?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != fs::metadata("/proc/self")?.uid()
    {
        return Err("escaped fixture records must be an owned actual directory".into());
    }
    let root = records.join("escaped");
    let _child = match std::env::args().nth(1).as_deref() {
        Some("--qa-logging-escaped") => {
            rustix::process::setsid()?;
            let leaf = spawn("--qa-logging-leaf")?;
            record(&root, "escaped")?;
            Some(leaf)
        }
        Some("--qa-logging-leaf") => {
            record(&root, "leaf")?;
            None
        }
        _ => {
            fs::create_dir(&root)?; // No reuse, alias or pre-existing fixture tree.
            let escaped = spawn("--qa-logging-escaped")?;
            return Ok((false, Some(escaped))); // Continue the unchanged actual logging probe body.
        }
    };
    // Independent finite lifetime even if the external test/controller fails.
    // Normal TERM is deliberately retained: the checker must still adopt/wait
    // this different-session child rather than kill only the producer's PGID.
    std::thread::sleep(Duration::from_secs(15));
    Ok((true, None))
}
