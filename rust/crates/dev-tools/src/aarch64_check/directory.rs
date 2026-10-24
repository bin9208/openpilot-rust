//! Retain the created directory's inode before any candidate executes. Identity
//! loss cannot grant authority to recursively delete a foreign replacement.
use crate::seal::Result;
use std::{
    fs::{self, File},
    os::unix::fs::MetadataExt,
    path::Path,
};
pub(super) struct Directory {
    temp: Option<tempfile::TempDir>,
    held: File,
    identity: (u64, u64),
}
impl Directory {
    pub(super) fn new(temp: tempfile::TempDir) -> Result<Self> {
        let held = File::open(temp.path())?;
        let metadata = held.metadata()?;
        Ok(Self {
            temp: Some(temp),
            held,
            identity: (metadata.dev(), metadata.ino()),
        })
    }
    pub(super) fn path(&self) -> &Path {
        self.temp.as_ref().expect("live owned directory").path()
    }
    fn owns_path(&self) -> bool {
        let Ok(now) = fs::symlink_metadata(self.path()) else {
            return false;
        };
        let Ok(held) = self.held.metadata() else {
            return false;
        };
        now.is_dir()
            && (now.dev(), now.ino()) == self.identity
            && (held.dev(), held.ino()) == self.identity
    }
    pub(super) fn close(mut self) -> Result<()> {
        if !self.owns_path() {
            let path = self.temp.take().expect("owned directory").keep();
            return Err(format!(
                "AArch64 owned directory identity changed: {}",
                path.display()
            )
            .into());
        }
        self.temp.take().expect("owned directory").close()?;
        Ok(())
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        if self.temp.is_some() {
            let owned = self.owns_path();
            if let Some(temp) = self.temp.take() {
                if owned {
                    let _ = temp.close();
                } else {
                    let _ = temp.keep();
                }
            }
        }
    }
}
