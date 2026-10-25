//! Atomic no-clobber reports with rollback bound to the exact published inode.
use super::check;
use crate::seal::Result;
use serde_json::Value;
use std::{
    fs::{self, File},
    io::Write,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
pub(super) struct Publication {
    path: PathBuf,
    device: u64,
    inode: u64,
    _file: File,
    committed: bool,
}
impl Publication {
    pub(super) fn write(path: &Path, value: &Value) -> Result<Self> {
        let mut temporary =
            tempfile::NamedTempFile::new_in(path.parent().ok_or("missing report parent")?)?;
        serde_json::to_writer_pretty(&mut temporary, value)?;
        temporary.write_all(b"\n")?;
        temporary.flush()?;
        let metadata = temporary.as_file().metadata()?;
        let file = temporary
            .persist_noclobber(path)
            .map_err(|error| error.error)?;
        Ok(Self {
            path: path.into(),
            device: metadata.dev(),
            inode: metadata.ino(),
            _file: file,
            committed: false,
        })
    }
    fn owns_path(&self) -> bool {
        fs::symlink_metadata(&self.path).is_ok_and(|metadata| {
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && (metadata.dev(), metadata.ino()) == (self.device, self.inode)
        })
    }
    pub(super) fn finish(mut self, cancel: &AtomicBool) -> Result<()> {
        check(cancel)?;
        if !self.owns_path() {
            return Err("bridge published report was replaced".into());
        }
        check(cancel)?;
        self.committed = true;
        Ok(())
    }
}
impl Drop for Publication {
    fn drop(&mut self) {
        if !self.committed && self.owns_path() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::Ordering;
    #[test]
    fn cancellation_after_publication_removes_owned_pass() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("result.json");
        let cancel = AtomicBool::new(false);
        let report = Publication::write(&path, &json!({"passed":true})).unwrap();
        assert!(path.exists());
        cancel.store(true, Ordering::Relaxed);
        assert!(report.finish(&cancel).is_err());
        assert!(!path.exists());
    }
    #[test]
    fn cancellation_does_not_remove_a_foreign_replacement() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("result.json");
        let report = Publication::write(&path, &json!({"passed":true})).unwrap();
        fs::rename(&path, root.path().join("old-report")).unwrap();
        fs::write(&path, b"foreign receipt").unwrap();
        assert!(report.finish(&AtomicBool::new(true)).is_err());
        assert_eq!(fs::read(path).unwrap(), b"foreign receipt");
    }
    #[test]
    fn failed_no_clobber_write_preserves_existing_receipt_and_success_commits() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("result.json");
        fs::write(&path, b"foreign receipt").unwrap();
        assert!(Publication::write(&path, &json!({"passed":true})).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"foreign receipt");
        fs::remove_file(&path).unwrap();
        let report = Publication::write(&path, &json!({"passed":true})).unwrap();
        report.finish(&AtomicBool::new(false)).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(path).unwrap()).unwrap(),
            json!({"passed":true})
        );
    }
}
