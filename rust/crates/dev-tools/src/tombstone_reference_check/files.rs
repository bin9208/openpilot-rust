//! Actual fixture filesystem observations and private descendant identities.
use super::json;
use crate::seal::Result;
use openpilot_log_format::JsonValue as J;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    os::unix::fs::PermissionsExt,
    path::Path,
    time::{Duration, Instant},
};
pub(super) struct Root(pub tempfile::TempDir);
impl Drop for Root {
    fn drop(&mut self) {
        fn restore(path: &Path) {
            if let Ok(meta) = fs::symlink_metadata(path) {
                if meta.is_dir() {
                    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
                    if let Ok(entries) = fs::read_dir(path) {
                        for entry in entries.flatten() {
                            restore(&entry.path());
                        }
                    }
                } else if meta.is_file() {
                    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
                }
            }
        }
        restore(self.0.path());
    }
}
pub(super) fn snapshot(root: &Path) -> Result<J> {
    fn visit(root: &Path, path: &Path, values: &mut Vec<(J, J)>) -> Result<()> {
        let meta = fs::metadata(path)?;
        let mode = meta.permissions().mode() & 0o7777;
        let value = if meta.is_dir() {
            json::object(&[
                ("directory", json::scalar("true")),
                ("mode", J::parse(&mode.to_string())?),
            ])?
        } else if path.is_symlink() {
            let target = fs::read_link(path)?;
            json::object(&[
                (
                    "symlink",
                    crate::python_fs::json_value(
                        target.file_name().ok_or("symlink target absent")?,
                    ),
                ),
                ("mode", J::parse(&mode.to_string())?),
            ])?
        } else {
            let mut file = fs::File::open(path)?;
            let mut hash = Sha256::new();
            let mut buf = [0; 65536];
            loop {
                let n = file.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                hash.update(&buf[..n]);
            }
            json::object(&[
                ("size", J::parse(&meta.len().to_string())?),
                ("sha256", J::text(&format!("{:x}", hash.finalize()))),
                ("mode", J::parse(&mode.to_string())?),
            ])?
        };
        values.push((
            crate::python_fs::json_value(path.strip_prefix(root)?.as_os_str()),
            value,
        ));
        Ok(())
    }
    let mut values = Vec::new();
    for folder in ["apport", "logs"] {
        let dir = root.join(folder);
        if dir.exists() {
            fn collect(path: &Path, paths: &mut Vec<std::path::PathBuf>) -> Result<()> {
                for entry in fs::read_dir(path)? {
                    let path = entry?.path();
                    if path.is_dir() && !path.is_symlink() {
                        collect(&path, paths)?;
                    }
                    paths.push(path);
                }
                Ok(())
            }
            let mut paths = Vec::new();
            collect(&dir, &mut paths)?;
            paths.sort_by_cached_key(|path| crate::python_fs::codepoints(path.as_os_str()));
            for path in paths {
                visit(root, &path, &mut values)?;
            }
        }
    }
    let parameter = root.join("params/fixture/CarrotException");
    let value = if parameter.is_file() {
        J::text(
            &fs::read(parameter)?
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
        )
    } else {
        json::scalar("null")
    };
    values.push((J::text("CarrotException"), value));
    Ok(J::parse(&format!(
        "{{{}}}",
        values
            .into_iter()
            .map(|(key, value)| Ok(format!("{}:{}", key.to_json()?, value.to_json()?)))
            .collect::<Result<Vec<_>>>()?
            .join(",")
    ))?)
}
/// Retrace deliberately creates a separate process group inside tombstone_trace.
/// Private environment tags let cleanup retain pidfds for those known children
/// without ever signalling an unreserved, reusable numeric process-group ID.
pub(super) fn cleanup_tagged(token: &Path) -> Result<()> {
    use rustix::{
        event::{poll, PollFd, PollFlags, Timespec},
        process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
    };
    let mut expected = b"TOMBSTONE_REFERENCE_OWNER=".to_vec();
    expected.extend_from_slice(token.as_os_str().as_encoded_bytes());
    let end = Instant::now() + Duration::from_secs(3);
    let mut quiet = 0;
    loop {
        let mut found = false;
        for entry in fs::read_dir("/proc")? {
            if Instant::now() >= end {
                return Err("owned tombstone descendant cleanup deadline".into());
            }
            let entry = entry?;
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|v| v.parse::<i32>().ok())
                .and_then(Pid::from_raw)
            else {
                continue;
            };
            let Ok(identity) = pidfd_open(pid, PidfdFlags::empty()) else {
                continue;
            };
            let Ok(mut file) = fs::File::open(entry.path().join("environ")) else {
                continue;
            };
            let mut env = Vec::new();
            if file
                .by_ref()
                .take(8 * 1024 * 1024)
                .read_to_end(&mut env)
                .is_err()
                || !env.split(|b| *b == 0).any(|v| v == expected)
            {
                continue;
            }
            found = true;
            let mut descriptor = [PollFd::new(&identity, PollFlags::IN)];
            if poll(&mut descriptor, Some(&Timespec::default()))? == 0 {
                let _ = pidfd_send_signal(&identity, Signal::KILL);
            }
        }
        if found {
            quiet = 0;
        } else {
            quiet += 1;
            if quiet == 2 {
                return Ok(());
            }
        }
        if Instant::now() >= end {
            return Err("owned tombstone descendants survived cleanup".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
pub(super) fn write(root: &Path, relative: &Path, bytes: &[u8], mode: Option<u32>) -> Result<()> {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().ok_or("fixture parent absent")?)?;
    fs::write(&path, bytes)?;
    if let Some(mode) = mode {
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    Ok(())
}

pub(super) fn stop_helpers(paths: &[std::path::PathBuf; 2]) -> Result<()> {
    use rustix::process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal};
    for path in paths {
        let raw = match fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        let pid = Pid::from_raw(raw.trim().parse()?).ok_or("invalid retrace PID")?;
        let Ok(identity) = pidfd_open(pid, PidfdFlags::empty()) else {
            continue;
        };
        let mut expected = b"FIXTURE_PID=".to_vec();
        expected.extend_from_slice(path.as_os_str().as_encoded_bytes());
        let Ok(env) = fs::read(format!("/proc/{}/environ", pid.as_raw_pid())) else {
            continue;
        };
        if env.split(|b| *b == 0).any(|entry| entry == expected) {
            let _ = pidfd_send_signal(&identity, Signal::KILL);
        }
    }
    Ok(())
}
