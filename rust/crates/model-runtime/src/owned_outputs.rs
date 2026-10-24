//! Roll back only regular files still naming the inode created by this writer.
//! Keep descriptors open until commit/drop, preventing reuse of owned inode IDs.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

#[derive(Default)]
pub struct Outputs {
    files: Vec<OwnedOutput>,
    committed: bool,
}
struct OwnedOutput {
    path: PathBuf,
    file: File,
}
impl OwnedOutput {
    fn still_owned(&self) -> io::Result<bool> {
        let owned = self.file.metadata()?;
        let current = match fs::symlink_metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        Ok(current.is_file() && current.dev() == owned.dev() && current.ino() == owned.ino())
    }
}
impl Outputs {
    pub fn write(&mut self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        // Anchor the parent before creation. Later parent-symlink changes must
        // not redirect cleanup to another directory.
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let name = path.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "output requires file name")
        })?;
        let path = fs::canonicalize(parent)?.join(name);
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        self.files.push(OwnedOutput { path, file });
        let output = self.files.last_mut().expect("inserted output");
        output.file.write_all(bytes)?;
        output.file.sync_all()
    }
    pub fn commit(mut self) -> io::Result<()> {
        for output in &self.files {
            if !output.still_owned()? {
                return Err(io::Error::other("output replaced before commit"));
            }
        }
        self.committed = true;
        Ok(())
    }
}
impl Drop for Outputs {
    fn drop(&mut self) {
        if !self.committed {
            for output in &self.files {
                // Inspect the link itself, never its target. A replacement or
                // an unreadable path is foreign and must be left alone.
                if output.still_owned().unwrap_or(false) {
                    let _ = fs::remove_file(&output.path);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cleanup_remains_anchored_when_parent_symlink_changes() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first");
        let second = root.path().join("second");
        fs::create_dir(&first).unwrap();
        fs::create_dir(&second).unwrap();
        let link = root.path().join("link");
        std::os::unix::fs::symlink(&first, &link).unwrap();
        let mut outputs = Outputs::default();
        outputs.write(&link.join("output"), b"owned").unwrap();
        fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink(&second, &link).unwrap();
        fs::write(second.join("output"), b"foreign").unwrap();
        drop(outputs);
        assert!(!first.join("output").exists());
        assert_eq!(fs::read(second.join("output")).unwrap(), b"foreign");
    }
    #[test]
    fn replacement_before_commit_is_an_error_and_survives() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("output");
        let mut outputs = Outputs::default();
        outputs.write(&path, b"owned").unwrap();
        fs::rename(&path, dir.path().join("original")).unwrap();
        fs::write(&path, b"foreign").unwrap();
        assert!(outputs.commit().is_err());
        assert_eq!(fs::read(path).unwrap(), b"foreign");
    }
    #[test]
    fn preexisting_output_and_earlier_owned_cleanup_on_late_collision() {
        let dir = tempfile::tempdir().unwrap();
        let owned = dir.path().join("owned");
        let foreign = dir.path().join("foreign");
        fs::write(&foreign, b"foreign").unwrap();
        let mut outputs = Outputs::default();
        outputs.write(&owned, b"owned").unwrap();
        assert_eq!(
            outputs.write(&foreign, b"replacement").unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        drop(outputs);
        assert!(!owned.exists());
        assert_eq!(fs::read(foreign).unwrap(), b"foreign");
    }
}
