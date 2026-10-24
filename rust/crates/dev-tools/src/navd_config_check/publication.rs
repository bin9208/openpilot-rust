//! A new aggregate receipt is revoked on failure only while its inode is owned.
use crate::seal::Result;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};
pub(super) struct Receipt {
    path: PathBuf,
    file: File,
    dev: u64,
    ino: u64,
    committed: bool,
}
impl Receipt {
    pub(super) fn publish(path: &Path, bytes: &[u8]) -> Result<Self> {
        let file = OpenOptions::new().write(true).create_new(true).open(path)?;
        let metadata = file.metadata()?;
        let mut receipt = Self {
            path: path.into(),
            file,
            dev: metadata.dev(),
            ino: metadata.ino(),
            committed: false,
        };
        receipt.file.write_all(bytes)?;
        receipt.file.flush()?;
        Ok(receipt)
    }
    fn owns_path(&self) -> bool {
        fs::symlink_metadata(&self.path)
            .is_ok_and(|m| m.is_file() && (m.dev(), m.ino()) == (self.dev, self.ino))
    }
    pub(super) fn verify(&self) -> Result<()> {
        if self.owns_path() {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "navd report ownership changed during publication",
            )
            .into())
        }
    }
    pub(super) fn commit(mut self) -> Result<()> {
        self.verify()?;
        self.committed = true;
        Ok(())
    }
}
impl Drop for Receipt {
    fn drop(&mut self) {
        if !self.committed && self.owns_path() {
            let _ = fs::remove_file(&self.path);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uncommitted_receipt_is_revoked_but_committed_receipt_is_retained() {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("report");
        drop(Receipt::publish(&path, b"pending").unwrap());
        assert!(!path.exists());
        Receipt::publish(&path, b"complete")
            .unwrap()
            .commit()
            .unwrap();
        assert_eq!(fs::read(path).unwrap(), b"complete");
    }
    #[test]
    fn publication_never_clobbers_a_preexisting_receipt() {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("report");
        fs::write(&path, b"foreign").unwrap();
        assert!(Receipt::publish(&path, b"owned").is_err());
        assert_eq!(fs::read(path).unwrap(), b"foreign");
    }
    #[test]
    fn replaced_receipt_is_rejected_and_never_removed_by_rollback() {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("report");
        let receipt = Receipt::publish(&path, b"owned").unwrap();
        fs::rename(&path, t.path().join("owned-moved")).unwrap();
        fs::write(&path, b"foreign").unwrap();
        assert!(receipt.commit().is_err());
        assert_eq!(fs::read(path).unwrap(), b"foreign");
        assert_eq!(fs::read(t.path().join("owned-moved")).unwrap(), b"owned");
    }
}
