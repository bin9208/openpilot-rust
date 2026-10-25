//! Parent-owned directories contain QA IPC before any child bind. Only the
//! creator removes a directory, after checking its held inode. Replaced roots
//! are preserved. Removal is not atomic against an actively racing same-UID peer.
use super::{require, Result};
use serde::{Deserialize, Serialize};
use std::os::unix::fs::PermissionsExt;
use std::{
    fs::{self, File},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};
const RECEIPT: &str = "UPLOADER_QA_NAMESPACE";
struct Directory {
    path: PathBuf,
    held: File,
    cleanup: bool,
}
impl Directory {
    fn capture(path: &Path) -> Result<Self> {
        let fd = rustix::fs::open(
            path,
            rustix::fs::OFlags::PATH | rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        let held = File::from(fd);
        require(
            held.metadata()?.is_dir(),
            "uploader owned directory type changed",
        )?;
        Ok(Self {
            path: path.into(),
            held,
            cleanup: false,
        })
    }
    fn identity(&self) -> Result<(u64, u64)> {
        let m = self.held.metadata()?;
        Ok((m.dev(), m.ino()))
    }
    fn admitted(path: &Path, expected: (u64, u64)) -> Result<Self> {
        let dir = Self::capture(path)?;
        require(
            dir.identity()? == expected,
            "uploader namespace replaced before worker admission",
        )?;
        require(
            dir.matches()?,
            "uploader namespace changed during admission",
        )?;
        Ok(dir)
    }
    fn matches(&self) -> Result<bool> {
        match fs::symlink_metadata(&self.path) {
            Ok(m) => Ok(self.identity()? == (m.dev(), m.ino())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
    fn created(path: &Path) -> Result<Self> {
        let mut dir = Self::capture(path)?;
        dir.cleanup = true;
        Ok(dir)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        if self.cleanup && self.matches().unwrap_or(false) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    prefix: String,
    shm: (u64, u64),
    ipc: PathBuf,
    ipc_identity: (u64, u64),
}
pub(crate) struct Namespace {
    pub(crate) shm: PathBuf,
    pub(crate) ipc_root: PathBuf,
    pub(crate) endpoint: PathBuf,
    directory: Directory,
    ipc: Directory,
}
impl Namespace {
    fn check_prefix(prefix: &str) -> Result<()> {
        require(
            prefix.starts_with("rust-probe-uploader-")
                && prefix.len() < 90
                && prefix
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-'),
            "unsafe uploader namespace prefix",
        )
    }
    pub(crate) fn new(prefix: &str) -> Result<Self> {
        Self::check_prefix(prefix)?;
        let shm = PathBuf::from(format!("/dev/shm/msgq_{prefix}"));
        // Create the private directory before starting any native worker. The
        // outer owner retains this descriptor throughout every child lifetime.
        fs::create_dir(&shm)?;
        let directory = Directory::created(&shm)?;
        let temp = tempfile::Builder::new()
            .prefix("route-upload-ipc-")
            .tempdir_in("/tmp")?;
        let ipc_root = temp.keep();
        let ipc = Directory::created(&ipc_root)?;
        Ok(Self {
            endpoint: ipc_root.join("logmessage"),
            shm,
            ipc_root,
            directory,
            ipc,
        })
    }
    pub(crate) fn worker(prefix: &str) -> Result<Self> {
        Self::check_prefix(prefix)?;
        let Some(raw) = std::env::var_os(RECEIPT) else {
            return Self::new(prefix);
        };
        let raw = raw
            .to_str()
            .ok_or("uploader namespace receipt is not UTF-8")?;
        require(raw.len() <= 4096, "uploader namespace receipt oversized")?;
        let receipt: Receipt = serde_json::from_str(raw)?;
        Self::admitted(prefix, receipt)
    }
    fn admitted(prefix: &str, receipt: Receipt) -> Result<Self> {
        require(
            receipt.prefix == prefix,
            "uploader namespace receipt prefix changed",
        )?;
        let root = &receipt.ipc;
        require(
            root.parent() == Some(Path::new("/tmp"))
                && root
                    .file_name()
                    .is_some_and(|v| v.as_encoded_bytes().starts_with(b"route-upload-ipc-")),
            "uploader IPC root is not a private QA directory",
        )?;
        let shm = PathBuf::from(format!("/dev/shm/msgq_{prefix}"));
        let directory = Directory::admitted(&shm, receipt.shm)?;
        let ipc = Directory::admitted(root, receipt.ipc_identity)?;
        require(
            fs::canonicalize(root)? == *root,
            "uploader IPC root spelling changed",
        )?;
        Ok(Self {
            endpoint: root.join("logmessage"),
            shm,
            ipc_root: root.clone(),
            directory,
            ipc,
        })
    }
    pub(crate) fn environment(&self) -> Result<[(String, String); 2]> {
        let prefix = self
            .shm
            .file_name()
            .and_then(|v| v.to_str())
            .and_then(|v| v.strip_prefix("msgq_"))
            .ok_or("uploader msgq basename absent")?;
        Ok([
            (
                RECEIPT.into(),
                serde_json::to_string(&Receipt {
                    prefix: prefix.into(),
                    shm: self.directory.identity()?,
                    ipc: self.ipc_root.clone(),
                    ipc_identity: self.ipc.identity()?,
                })?,
            ),
            (
                "ROUTE_UPLOAD_QA_IPC_ROOT".into(),
                self.ipc_root
                    .to_str()
                    .ok_or("uploader IPC root not UTF-8")?
                    .into(),
            ),
        ])
    }
}
pub(crate) struct Permission(fs::File, bool);
impl Permission {
    fn new(path: &Path) -> Result<Self> {
        let fd = rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        Ok(Self(fs::File::from(fd), false))
    }
    pub(crate) fn created(path: &Path) -> Result<Self> {
        let mut p = Self::new(path)?;
        p.1 = true;
        Ok(p)
    }
    pub(crate) fn admitted(path: &Path, expected: (u64, u64)) -> Result<Self> {
        let mut p = Self::new(path)?;
        require(
            p.identity()? == expected,
            "sibling fixture root replaced before worker admission",
        )?;
        p.1 = true;
        Ok(p)
    }
    pub(crate) fn mode(&self, mode: u32) -> Result<()> {
        self.0.set_permissions(fs::Permissions::from_mode(mode))?;
        Ok(())
    }
    pub(crate) fn identity(&self) -> Result<(u64, u64)> {
        use std::os::unix::fs::MetadataExt;
        let m = self.0.metadata()?;
        Ok((m.dev(), m.ino()))
    }
}
impl Drop for Permission {
    fn drop(&mut self) {
        if !self.1 {
            return;
        }
        let _ = self.0.set_permissions(fs::Permissions::from_mode(0o755));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::{
        fs::{symlink, FileTypeExt},
        net::UnixListener,
    };
    #[test]
    fn owned_root_cleanup_preserves_same_name_directory_and_symlink_replacements() {
        for link in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("owned");
            fs::create_dir(&path).unwrap();
            let owned = Directory::created(&path).unwrap();
            fs::rename(&path, root.path().join("original")).unwrap();
            let foreign = root.path().join("foreign");
            fs::create_dir(&foreign).unwrap();
            fs::write(foreign.join("keep"), b"foreign").unwrap();
            if link {
                symlink(&foreign, &path).unwrap();
            } else {
                fs::create_dir(&path).unwrap();
                fs::write(path.join("keep"), b"replacement").unwrap();
            }
            drop(owned);
            assert_eq!(fs::read(foreign.join("keep")).unwrap(), b"foreign");
            if link {
                assert!(fs::symlink_metadata(&path)
                    .unwrap()
                    .file_type()
                    .is_symlink());
            } else {
                assert_eq!(fs::read(path.join("keep")).unwrap(), b"replacement");
            }
        }
    }
    #[test]
    fn rejected_admission_has_no_authority_over_foreign_directory_or_symlink() {
        for link in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("owned");
            fs::create_dir(&path).unwrap();
            let original = Directory::created(&path).unwrap();
            let identity = original.identity().unwrap();
            fs::rename(&path, root.path().join("original")).unwrap();
            let foreign = root.path().join("foreign");
            fs::create_dir(&foreign).unwrap();
            fs::write(foreign.join("keep"), b"foreign").unwrap();
            if link {
                symlink(&foreign, &path).unwrap();
            } else {
                fs::create_dir(&path).unwrap();
                fs::write(path.join("keep"), b"replacement").unwrap();
            }
            assert!(Directory::admitted(&path, identity).is_err());
            drop(original);
            assert_eq!(fs::read(foreign.join("keep")).unwrap(), b"foreign");
            assert!(fs::symlink_metadata(&path).is_ok());
        }
    }
    #[test]
    fn borrowed_worker_cannot_cleanup_parent_root_or_unowned_global_socket() {
        let private = tempfile::tempdir().unwrap();
        let token = private
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .replace('.', "-");
        let prefix = format!("rust-probe-uploader-life-{}-{token}", std::process::id());
        let global = PathBuf::from(format!("/tmp/logmessage{prefix}"));
        let sentinel = UnixListener::bind(&global).unwrap();
        let owner = Namespace::new(&prefix).unwrap();
        assert!(Namespace::new(&prefix).is_err());
        let env = owner.environment().unwrap();
        let receipt: Receipt = serde_json::from_str(&env[0].1).unwrap();
        let worker = Namespace::admitted(&prefix, receipt).unwrap();
        let socket = UnixListener::bind(&worker.endpoint).unwrap();
        let shm = owner.shm.clone();
        let ipc = owner.ipc_root.clone();
        drop(worker);
        assert!(shm.is_dir() && ipc.is_dir());
        drop(socket);
        drop(owner);
        assert!(!shm.exists() && !ipc.exists());
        assert!(fs::symlink_metadata(&global)
            .unwrap()
            .file_type()
            .is_socket());
        drop(sentinel);
        fs::remove_file(global).unwrap();
    }
}
