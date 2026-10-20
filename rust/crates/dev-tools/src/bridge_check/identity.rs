//! Opened regular-file identities; preflight never waits for a FIFO writer.
use crate::seal::Result;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{self, Read},
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
};
struct Artifact {
    // Holding the original descriptor also prevents inode reuse during checks.
    _file: File,
    device: u64,
    inode: u64,
    sha256: String,
}
impl Artifact {
    fn open(path: &Path) -> Result<Self> {
        let mut file = File::from(rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "bridge executable is not a regular file",
            )
            .into());
        }
        let mut hash = Sha256::new();
        let mut bytes = [0; 65536];
        loop {
            let count = file.read(&mut bytes)?;
            if count == 0 {
                break;
            }
            hash.update(&bytes[..count]);
        }
        Ok(Self {
            _file: file,
            device: metadata.dev(),
            inode: metadata.ino(),
            sha256: format!("{:x}", hash.finalize()),
        })
    }
    fn verify(&self, path: &Path) -> Result<()> {
        let current = Self::open(path)?;
        if (self.device, self.inode, &self.sha256)
            != (current.device, current.inode, &current.sha256)
        {
            return Err(format!(
                "bridge executable changed during comparison: {}",
                path.display()
            )
            .into());
        }
        Ok(())
    }
}
/// Only the historical Python controller has an absence exception. A present
/// copy must still match its original bytes and remain the same regular file.
pub(super) struct HistoricalSource {
    path: PathBuf,
    present: Option<Artifact>,
    expected_sha256: String,
}
impl HistoricalSource {
    pub(super) fn capture(path: &Path, expected_sha256: &str) -> Result<Self> {
        let parent = path
            .parent()
            .ok_or("bridge historical controller parent absent")?;
        if !fs::metadata(parent)?.is_dir() {
            return Err("bridge historical controller parent is not a directory".into());
        }
        let present = match fs::symlink_metadata(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
            Ok(metadata) if metadata.file_type().is_file() => {
                let artifact = Artifact::open(path)?;
                if artifact.sha256 != expected_sha256 {
                    return Err("bridge historical controller source drift".into());
                }
                Some(artifact)
            }
            Ok(_) => return Err("bridge historical controller is not a regular file".into()),
        };
        Ok(Self {
            path: path.into(),
            present,
            expected_sha256: expected_sha256.into(),
        })
    }
    pub(super) fn verify(&self) -> Result<()> {
        let parent = self
            .path
            .parent()
            .ok_or("bridge historical controller parent absent")?;
        if !fs::metadata(parent)?.is_dir() {
            return Err("bridge historical controller parent is not a directory".into());
        }
        match &self.present {
            Some(artifact) => {
                if !fs::symlink_metadata(&self.path)?.file_type().is_file() {
                    return Err("bridge historical controller changed file type".into());
                }
                artifact.verify(&self.path)?;
                if artifact.sha256 != self.expected_sha256 {
                    return Err("bridge historical controller source drift".into());
                }
            }
            None => match fs::symlink_metadata(&self.path) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
                Ok(_) => return Err("bridge historical controller reappeared".into()),
            },
        }
        Ok(())
    }
}
/// An unreadable/nonregular input still reaches the original child-spawn phase.
/// A successful comparison, however, requires a retained regular-file identity.
/// This detects bounded before/after replacement, not arbitrary ABA mutation.
pub(super) struct Input {
    path: PathBuf,
    artifact: Option<Artifact>,
}
impl Input {
    pub(super) fn capture(path: &Path) -> Self {
        Self {
            path: path.into(),
            artifact: Artifact::open(path).ok(),
        }
    }
    pub(super) fn verify_before(&self) -> Result<()> {
        if let Some(artifact) = &self.artifact {
            artifact.verify(&self.path)?;
        }
        Ok(())
    }
    pub(super) fn verified_sha256(&self) -> Result<&str> {
        let artifact = self.artifact.as_ref().ok_or_else(|| {
            format!(
                "successful bridge input has no regular-file identity: {}",
                self.path.display()
            )
        })?;
        artifact.verify(&self.path)?;
        Ok(&artifact.sha256)
    }
}
/// serde's Path serializer rejects surrogateescape filenames. Preserve every
/// Unix byte in an explicit diagnostic representation instead of lossy text.
pub(super) fn path_value(path: &Path) -> Value {
    match path.to_str() {
        Some(text) => json!(text),
        None => json!({"unix_bytes":path.as_os_str().as_bytes()}),
    }
}
pub(super) fn invocation(binary: &Path, scenario: &str, parent: &Path) -> Value {
    json!({"binary":path_value(binary),"scenario":scenario,"parent_network":path_value(parent),"timeout_seconds":60})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::ffi::OsStringExt;
    #[test]
    fn fifo_is_rejected_without_waiting_for_a_writer_and_preflight_is_deferred() {
        let root = tempfile::tempdir().unwrap();
        let fifo = root.path().join("fifo");
        rustix::fs::mkfifoat(
            rustix::fs::CWD,
            &fifo,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        )
        .unwrap();
        assert!(Artifact::open(&fifo).is_err());
        let input = Input::capture(&fifo);
        assert!(input.artifact.is_none());
        assert!(input.verify_before().is_ok());
        assert!(input.verified_sha256().is_err());
    }
    #[test]
    fn held_regular_identity_rejects_same_bytes_replacement_and_in_place_drift() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("binary");
        fs::write(&path, b"original content").unwrap();
        let input = Input::capture(&path);
        assert!(input.verified_sha256().is_ok());
        let moved = root.path().join("held-original");
        fs::rename(&path, &moved).unwrap();
        fs::write(&path, b"original content").unwrap();
        assert!(input.verify_before().is_err());
        assert!(input.verified_sha256().is_err());
        let input = Input::capture(&path);
        fs::write(&path, b"different content").unwrap();
        assert!(input.verified_sha256().is_err());
    }
    #[test]
    fn raw_filename_metadata_serializes_without_losing_any_byte() {
        let bytes = b"/owned/bridge-\xff-\xc2\xa0";
        let path = PathBuf::from(std::ffi::OsString::from_vec(bytes.to_vec()));
        let value = invocation(&path, "outgoing", Path::new("net:[123]"));
        let encoded = serde_json::to_vec(&value).unwrap();
        let decoded: Value = serde_json::from_slice(&encoded).unwrap();
        let actual = decoded["binary"]["unix_bytes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b.as_u64().unwrap() as u8)
            .collect::<Vec<_>>();
        assert_eq!(actual, bytes);
        assert_eq!(path_value(Path::new("/owned/ascii")), json!("/owned/ascii"));
    }
    #[test]
    fn exact_historical_source_absence_and_reappearance_are_distinct_from_drift() {
        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("tools");
        let path = parent.join("check_bridge.py");
        let expected = format!("{:x}", Sha256::digest(b"original source"));
        assert!(HistoricalSource::capture(&path, &expected).is_err());
        fs::create_dir(&parent).unwrap();
        let absent = HistoricalSource::capture(&path, &expected).unwrap();
        absent.verify().unwrap();
        fs::remove_dir(&parent).unwrap();
        assert!(absent.verify().is_err(), "parent removal must fail");
        fs::create_dir(&parent).unwrap();
        fs::write(&path, b"original source").unwrap();
        assert!(absent.verify().is_err(), "reappearance must fail");
        let present = HistoricalSource::capture(&path, &expected).unwrap();
        present.verify().unwrap();
        fs::write(&path, b"changed source").unwrap();
        assert!(present.verify().is_err(), "in-place drift must fail");
        assert!(HistoricalSource::capture(&path, &expected).is_err());
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink("missing", &path).unwrap();
        assert!(HistoricalSource::capture(&path, &expected).is_err());
        fs::remove_file(&path).unwrap();
        rustix::fs::mkfifoat(
            rustix::fs::CWD,
            &path,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        )
        .unwrap();
        assert!(HistoricalSource::capture(&path, &expected).is_err());
    }
}
