//! Serialize selected-model delivery before boot admission and manager startup.
use super::{pause, Paths};
use crate::{watchdog::Server, Error};
use std::{
    env,
    fs::{File, OpenOptions, TryLockError},
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

fn command(paths: &Paths, mode: &str) -> Command {
    let mut command = Command::new(paths.binary("openpilot-usbgpu-model"));
    command.arg(mode);
    if mode != "--present" {
        command.arg("--root").arg(&paths.repository);
    }
    command
        .arg("--devices")
        .arg(paths.system.join("sys/bus/usb/devices"))
        .env_remove("CARROT_BIG_MODEL_STARTUP_FAILED")
        .current_dir(&paths.repository);
    command
}

/// Returns true only for a terminal delivery failure. Cancellation never starts manager.
pub(super) fn prepare(paths: &Paths, stopped: &AtomicBool) -> Result<bool, Error> {
    if !Server(command(paths, "--present").spawn()?)
        .finish(stopped)?
        .success()
        || stopped.load(Ordering::Relaxed)
    {
        return Ok(false);
    }
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(paths.temporary.join("big_model_update.lock"))?;
    loop {
        if stopped.load(Ordering::Relaxed) {
            return Ok(false);
        }
        match lock.try_lock() {
            Ok(()) => break,
            Err(TryLockError::WouldBlock) => pause(Duration::from_millis(50), stopped),
            Err(TryLockError::Error(error)) => return Err(error.into()),
        }
    }
    let mut spinner = Spinner::new(paths);
    spinner.update("USB eGPU model\nchecking model catalog");
    let mut child = Server(
        command(paths, "--prepare-for-startup")
            .arg("--retry-network")
            .spawn()?,
    );
    let mut refreshed = Instant::now();
    while !stopped.load(Ordering::Relaxed) {
        if let Some(status) = child.0.try_wait()? {
            if !status.success() {
                eprintln!(
                    "Selected eGPU model preparation failed; using the internal model this boot."
                );
            }
            return Ok(!status.success());
        }
        if refreshed.elapsed() >= Duration::from_millis(250) {
            if let Some(text) = progress(paths) {
                spinner.update(&text);
            }
            refreshed = Instant::now();
        }
        pause(Duration::from_millis(25), stopped);
    }
    child.finish(stopped)?;
    Ok(false)
}

fn progress(paths: &Paths) -> Option<String> {
    let cache = env::var_os("CARROT_BIG_MODEL_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            paths
                .system
                .join("TICI")
                .is_file()
                .then(|| paths.data.join("media/0/carrot/models"))
        })
        .or_else(|| home::home_dir().map(|p| p.join(".comma/models")))?;
    let mut bytes = Vec::new();
    File::open(cache.join("status.json"))
        .ok()?
        .take(32769)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 32768 {
        return None;
    }
    status_text(&serde_json::from_slice(&bytes).ok()?)
}

fn status_text(value: &serde_json::Value) -> Option<String> {
    if value.get("schema_version")?.as_u64()? != 1 {
        return None;
    }
    let state = value.get("state")?.as_str()?;
    let detail = if state == "waiting_for_network" {
        format!(
            "Waiting for network ({}); retrying in 30s",
            value
                .get("error_code")
                .and_then(|v| v.as_str())
                .unwrap_or("network")
        )
    } else if state == "downloading"
        && value
            .get("total_bytes")
            .and_then(|v| v.as_u64())
            .is_some_and(|v| v > 0)
    {
        let total = u128::from(value["total_bytes"].as_u64()?);
        let done = u128::from(
            value
                .get("downloaded_bytes")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
        );
        format!("Downloading {}%", 100 * done / total)
    } else {
        value
            .get("detail")
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
            .map_or_else(|| state.replace('_', " "), str::to_owned)
    };
    Some(format!("USB eGPU model\n{detail}"))
}

// Best-effort UI is bounded independently of delivery. A stalled renderer cannot
// fill a pipe and prevent retries/cancellation; restart at most once, like source.
struct Spinner {
    binary: PathBuf,
    root: PathBuf,
    child: Option<Server>,
    attempts: u8,
}
impl Spinner {
    fn new(paths: &Paths) -> Self {
        Self {
            binary: paths.binary("openpilot-spinner"),
            root: paths.repository.clone(),
            child: None,
            attempts: 0,
        }
    }
    fn start(&mut self) -> std::io::Result<()> {
        if self.attempts >= 2 {
            return Err(std::io::Error::other("spinner restart limit"));
        }
        self.attempts += 1;
        let child = Command::new(&self.binary)
            .arg("--source-root")
            .arg(&self.root)
            .stdin(Stdio::piped())
            .spawn()?;
        let child = Server(child);
        let pipe = child
            .0
            .stdin
            .as_ref()
            .ok_or_else(|| std::io::Error::other("spinner stdin missing"))?;
        let flags = rustix::fs::fcntl_getfl(pipe)?;
        rustix::fs::fcntl_setfl(pipe, flags | rustix::fs::OFlags::NONBLOCK)?;
        self.child = Some(child);
        Ok(())
    }
    fn update(&mut self, text: &str) {
        // Keep each write within Linux PIPE_BUF, including the delimiter.
        let mut payload = text.chars().take(1000).collect::<String>();
        payload.push('\n');
        for _ in 0..2 {
            let alive = self
                .child
                .as_mut()
                .is_some_and(|child| matches!(child.0.try_wait(), Ok(None)));
            if !alive {
                self.child = None;
                if self.start().is_err() {
                    return;
                }
            }
            let result = self
                .child
                .as_mut()
                .and_then(|c| c.0.stdin.as_mut())
                .map(|p| p.write_all(payload.as_bytes()));
            match result {
                Some(Ok(())) => return,
                Some(Err(error)) if error.kind() == std::io::ErrorKind::WouldBlock => return,
                _ => self.child = None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_status_labels_preserve_network_and_integer_progress() {
        use serde_json::json;
        for (value, expected) in [
            (json!({"state":"checking"}), "checking"),
            (
                json!({"state":"waiting_for_network","error_code":"dns"}),
                "Waiting for network (dns); retrying in 30s",
            ),
            (
                json!({"state":"downloading","downloaded_bytes":2,"total_bytes":3}),
                "Downloading 66%",
            ),
            (
                json!({"state":"installing","detail":"verified model"}),
                "verified model",
            ),
            (
                json!({"state":"waiting_for_ignition","detail":""}),
                "waiting for ignition",
            ),
        ] {
            let mut value = value;
            value["schema_version"] = json!(1);
            assert_eq!(
                status_text(&value),
                Some(format!("USB eGPU model\n{expected}"))
            );
        }
        assert_eq!(
            status_text(&json!({"schema_version":2,"state":"ready"})),
            None
        );
    }
}
