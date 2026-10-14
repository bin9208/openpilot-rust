//! Native service ownership is established by the executable, never argv text.
use rustix::process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal};
use std::{
    fs, io,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, PartialEq, Eq)]
struct Identity {
    pid: i32,
    start: u64,
    device: u64,
    inode: u64,
    executable: PathBuf,
}

fn snapshot(pid: i32) -> io::Result<Option<Identity>> {
    let directory = PathBuf::from(format!("/proc/{pid}"));
    let stat = match fs::read_to_string(directory.join("stat")) {
        Ok(stat) => stat,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let fields: Vec<_> = stat
        .rsplit_once(')')
        .ok_or_else(|| io::Error::other("invalid process stat"))?
        .1
        .split_whitespace()
        .collect();
    if fields
        .first()
        .is_some_and(|state| *state == "Z" || *state == "X")
    {
        return Ok(None);
    }
    let start = fields
        .get(19)
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| io::Error::other("missing process start time"))?;
    let executable = match fs::read_link(directory.join("exe")) {
        Ok(path) => path,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let metadata = match fs::metadata(directory.join("exe")) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    // Reading a second starttime rejects PID reuse while collecting exe metadata.
    let later = match fs::read_to_string(directory.join("stat")) {
        Ok(later) => later,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if later
        .rsplit_once(')')
        .and_then(|(_, tail)| tail.split_whitespace().nth(19))
        .and_then(|value| value.parse::<u64>().ok())
        != Some(start)
    {
        return Ok(None);
    }
    Ok(Some(Identity {
        pid,
        start,
        device: metadata.dev(),
        inode: metadata.ino(),
        executable,
    }))
}

pub(crate) fn matches(pid: i32, executable: &Path) -> io::Result<bool> {
    if pid <= 0 {
        return Ok(false);
    }
    let expected_path = match fs::canonicalize(executable) {
        Ok(path) => path,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    let expected = match fs::metadata(&expected_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    let actual = match snapshot(pid) {
        Ok(actual) => actual,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::PermissionDenied | io::ErrorKind::NotFound
            ) =>
        {
            return Ok(false)
        }
        Err(error) => return Err(error),
    };
    Ok(actual.is_some_and(|actual| {
        !old_service(&actual.executable, &expected_path)
            && (actual.device, actual.inode) == (expected.dev(), expected.ino())
    }))
}

fn generation(path: &Path) -> Option<(&Path, &Path)> {
    let bin = path.parent()?;
    let rust = bin.parent()?;
    let generation = rust.parent()?;
    let store = generation.parent()?;
    let name = generation.file_name()?.to_str()?;
    let parts: Vec<_> = name.split('-').collect();
    if bin.file_name()? != "bin"
        || rust.file_name()? != "rust"
        || store.file_name()? != ".runtime-packages"
        || parts.len() != 4
        || parts[0] != "gen"
        || parts[1].len() != 40
        || !parts[1].bytes().all(|b| b.is_ascii_hexdigit())
        || parts[2].len() != 64
        || !parts[2].bytes().all(|b| b.is_ascii_hexdigit())
        || parts[3].is_empty()
        || parts[3].len() > 32
        || !parts[3].bytes().all(|b| b.is_ascii_alphanumeric())
    {
        return None;
    }
    Some((store, path.strip_prefix(generation).ok()?))
}

fn without_deleted(path: &Path) -> &Path {
    use std::os::unix::ffi::OsStrExt;
    let bytes = path.as_os_str().as_bytes();
    Path::new(std::ffi::OsStr::from_bytes(
        bytes.strip_suffix(b" (deleted)").unwrap_or(bytes),
    ))
}

fn old_service(actual: &Path, expected: &Path) -> bool {
    let actual = without_deleted(actual);
    actual != expected
        && generation(actual)
            .zip(generation(expected))
            .is_some_and(|(old, new)| old == new)
}

fn retire(identity: &Identity) -> io::Result<()> {
    let pid = Pid::from_raw(identity.pid).ok_or_else(|| io::Error::other("invalid service PID"))?;
    let fd = match pidfd_open(pid, PidfdFlags::empty()) {
        Ok(fd) => fd,
        Err(rustix::io::Errno::SRCH) => return Ok(()),
        Err(error) => {
            return Err(io::Error::other(format!(
                "cannot safely retire old runtime service {}: {error}; reboot required",
                identity.pid
            )))
        }
    };
    for (signal, timeout) in [
        (Signal::TERM, Duration::from_secs(2)),
        (Signal::KILL, Duration::from_secs(1)),
    ] {
        if snapshot(identity.pid)?.as_ref() != Some(identity) {
            return Ok(());
        }
        match pidfd_send_signal(&fd, signal) {
            Ok(()) => (),
            Err(rustix::io::Errno::SRCH) => return Ok(()),
            Err(error) => return Err(error.into()),
        }
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if snapshot(identity.pid)?.as_ref() != Some(identity) {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
    Err(io::Error::other(
        "old native service did not exit; reboot required",
    ))
}

/// Reuse the exact installed executable, retiring verified same-store old generations.
/// Kernels without pidfds require a reboot for safe old-generation replacement.
pub fn reconcile_native_service(executable: &Path) -> io::Result<bool> {
    let expected = fs::canonicalize(executable)?;
    let metadata = fs::metadata(&expected)?;
    let mut current = false;
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|value| value.parse::<i32>().ok())
        else {
            continue;
        };
        if pid == std::process::id() as i32 {
            continue;
        }
        let actual = match snapshot(pid) {
            Ok(Some(actual)) => actual,
            Ok(None) => continue,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::PermissionDenied | io::ErrorKind::NotFound
                ) =>
            {
                continue
            }
            Err(error) => return Err(error),
        };
        if old_service(&actual.executable, &expected) {
            retire(&actual)?;
        } else if (actual.device, actual.inode) == (metadata.dev(), metadata.ino()) {
            current = true;
        }
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Child, Command};

    struct Owned(Child);
    impl Drop for Owned {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn spawn(path: &Path) -> Owned {
        fs::File::open(path).unwrap().sync_all().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let child = loop {
            match Command::new(path).arg("30").spawn() {
                Ok(child) => break Owned(child),
                // Another parallel test can fork while fs::copy still holds
                // its writer FD; wait only for that known fixture condition.
                Err(error)
                    if error.raw_os_error() == Some(rustix::io::Errno::TXTBSY.raw_os_error())
                        && Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(error) => panic!("native fixture spawn failed: {error}"),
            }
        };
        let metadata = fs::metadata(path).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while snapshot(child.0.id() as i32)
            .unwrap()
            .is_none_or(|actual| actual.inode != metadata.ino())
        {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        child
    }

    fn binary(root: &Path, nonce: &str) -> PathBuf {
        let path = root
            .join(".runtime-packages")
            .join(format!("gen-{}-{}-{nonce}", "a".repeat(40), "b".repeat(64)))
            .join("rust/bin/owned-service");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::copy("/bin/sleep", &path).unwrap();
        path
    }

    #[test]
    fn two_generations_retire_old_and_reuse_current_without_touching_another_store() {
        let root = tempfile::tempdir().unwrap();
        let unrelated_root = tempfile::tempdir().unwrap();
        let old_path = binary(root.path(), "old");
        let new_path = binary(root.path(), "new");
        let unrelated_path = binary(unrelated_root.path(), "old");
        let mut old = spawn(&old_path);
        let mut unrelated = spawn(&unrelated_path);
        assert!(!reconcile_native_service(&new_path).unwrap());
        assert!(old.0.wait().unwrap().code().is_none());
        assert!(unrelated.0.try_wait().unwrap().is_none());
        let mut current = spawn(&new_path);
        assert!(reconcile_native_service(&new_path).unwrap());
        assert!(current.0.try_wait().unwrap().is_none());
    }

    #[test]
    fn shared_inode_does_not_make_an_old_generation_current() {
        let root = tempfile::tempdir().unwrap();
        let old_path = binary(root.path(), "old");
        let new_path = binary(root.path(), "new");
        fs::remove_file(&new_path).unwrap();
        fs::hard_link(&old_path, &new_path).unwrap();
        let mut old = spawn(&old_path);
        assert!(!reconcile_native_service(&new_path).unwrap());
        assert!(old.0.wait().unwrap().code().is_none());
    }

    #[test]
    fn stale_start_time_does_not_signal_reused_pid() {
        let root = tempfile::tempdir().unwrap();
        let path = binary(root.path(), "old");
        let mut child = spawn(&path);
        let mut stale = snapshot(child.0.id() as i32).unwrap().unwrap();
        stale.start = stale.start.saturating_add(1);
        retire(&stale).unwrap();
        assert!(child.0.try_wait().unwrap().is_none());
    }

    #[test]
    fn deleted_old_executable_is_retired_by_its_original_generation() {
        let root = tempfile::tempdir().unwrap();
        let old_path = binary(root.path(), "old");
        let new_path = binary(root.path(), "new");
        let mut old = spawn(&old_path);
        fs::remove_file(&old_path).unwrap();
        assert!(!reconcile_native_service(&new_path).unwrap());
        assert!(old.0.wait().unwrap().code().is_none());
    }
}
