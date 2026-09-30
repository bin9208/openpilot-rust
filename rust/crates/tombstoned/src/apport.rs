//! External apport-retrace boundary; no Python interpreter is embedded or spawned by this crate.
use crate::Error;
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
};
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub const RETRACE_TIMEOUT: Duration = Duration::from_secs(30);
pub struct Retrace {
    pub shell: std::path::PathBuf,
    pub timeout: Duration,
}
impl Default for Retrace {
    fn default() -> Self {
        Self {
            shell: "/bin/bash".into(),
            timeout: RETRACE_TIMEOUT,
        }
    }
}
impl Retrace {
    /// Preserve the process-substitution pipeline, but pass the filename as an argument (#77).
    pub fn stacktrace(&self, path: &Path) -> Result<String, Error> {
        let mut child = Command::new(&self.shell)
            .args([
                "-c",
                "apport-retrace -s <(cat <(echo \"Package: openpilot\") \"$1\")",
                "apport-retrace",
            ])
            .arg(path)
            .stdin(Stdio::inherit())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let result = (|| {
            let mut stdout = child
                .stdout
                .take()
                .ok_or(Error::Contract("missing retrace stdout"))?;
            fcntl_setfl(&stdout, fcntl_getfl(&stdout)? | OFlags::NONBLOCK)?;
            let deadline = Instant::now()
                .checked_add(self.timeout)
                .ok_or(Error::Contract("timeout overflow"))?;
            let mut bytes = Vec::new();
            let mut eof = false;
            let mut status = None;
            loop {
                let mut buffer = [0_u8; 8192];
                loop {
                    if Instant::now() >= deadline {
                        break;
                    }
                    match stdout.read(&mut buffer) {
                        Ok(0) => {
                            eof = true;
                            break;
                        }
                        Ok(count) => bytes.extend_from_slice(&buffer[..count]),
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(error) => return Err(error.into()),
                    }
                }
                if status.is_none() {
                    status = child.try_wait()?;
                }
                if let Some(status) = status.filter(|_| eof) {
                    let text = String::from_utf8(bytes)?
                        .replace("\r\n", "\n")
                        .replace('\r', "\n");
                    return Ok(if status.success() {
                        text
                    } else {
                        "Error getting stacktrace".into()
                    });
                }
                let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                    if status.is_none() {
                        child.kill()?;
                        child.wait()?;
                    }
                    return Ok("Timeout getting stacktrace".into());
                };
                if eof {
                    std::thread::sleep(remaining.min(Duration::from_millis(10)));
                } else {
                    let timeout = Timespec {
                        tv_sec: i64::try_from(remaining.as_secs())
                            .map_err(|_| Error::Contract("timeout overflow"))?,
                        tv_nsec: i64::from(remaining.subsec_nanos()),
                    };
                    match poll(&mut [PollFd::new(&stdout, PollFlags::IN)], Some(&timeout)) {
                        Ok(_) | Err(rustix::io::Errno::INTR) => {}
                        Err(error) => return Err(error.into()),
                    }
                }
            }
        })();
        if result.is_err() && child.try_wait()?.is_none() {
            child.kill()?;
            child.wait()?;
        }
        result
    }
}
