//! Creator-held directory identity; replacements are preserved, not adopted.
//! Removal is not atomic against an actively racing same-UID process.
use super::{require, Result};
use std::{
    fs::{self, File},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};
pub struct Directory {
    pub path: PathBuf,
    held: File,
}
impl Directory {
    pub fn temporary() -> Result<Self> {
        let path = tempfile::Builder::new()
            .prefix("rust-deleter-qa-")
            .tempdir()?
            .keep();
        let held = File::from(rustix::fs::open(
            &path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        Ok(Self { path, held })
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        if let (Ok(a), Ok(b)) = (self.held.metadata(), fs::symlink_metadata(&self.path)) {
            if b.is_dir() && !b.file_type().is_symlink() && (a.dev(), a.ino()) == (b.dev(), b.ino())
            {
                let _ = fs::remove_dir_all(&self.path);
            }
        }
    }
}
pub struct Permission(File, u32);
impl Permission {
    pub fn deny(path: &Path) -> Result<Self> {
        let file = File::from(rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        require(
            file.metadata()?.is_dir(),
            "deleter permission target is not directory",
        )?;
        let mode = file.metadata()?.permissions().mode();
        let guard = Self(file, mode);
        guard.0.set_permissions(fs::Permissions::from_mode(0o0))?;
        Ok(guard)
    }
}
impl Drop for Permission {
    fn drop(&mut self) {
        let _ = self.0.set_permissions(fs::Permissions::from_mode(self.1));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    #[test]
    fn held_cleanup_preserves_same_name_foreign_directory_and_symlink() {
        for link in [false, true] {
            let parent = tempfile::tempdir().unwrap();
            let original = Directory::temporary().unwrap();
            let path = original.path.clone();
            let old = parent.path().join("old");
            fs::rename(&path, &old).unwrap();
            let foreign = parent.path().join("foreign");
            fs::create_dir(&foreign).unwrap();
            fs::write(foreign.join("keep"), b"foreign").unwrap();
            if link {
                symlink(&foreign, &path).unwrap()
            } else {
                fs::create_dir(&path).unwrap();
                fs::write(path.join("keep"), b"replacement").unwrap()
            }
            drop(original);
            assert_eq!(fs::read(foreign.join("keep")).unwrap(), b"foreign");
            assert!(fs::symlink_metadata(&path).is_ok());
            if link {
                fs::remove_file(path).unwrap()
            } else {
                fs::remove_dir_all(path).unwrap()
            }
        }
    }
    #[test]
    fn permission_restore_uses_original_descriptor_after_replacement() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("owned");
        fs::create_dir(&path).unwrap();
        let permission = Permission::deny(&path).unwrap();
        fs::rename(&path, root.path().join("old")).unwrap();
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o711)).unwrap();
        drop(permission);
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o711);
    }
}
