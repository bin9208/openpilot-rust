//! A shared runtime leaf is removable only after exact kernel VFS association.
use super::{require, Result};
use std::{
    fs::{self, File},
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::{Path, PathBuf},
};
#[path = "../route_upload_check/lifecycle/socket_identity.rs"]
mod socket_identity;

pub(super) struct SocketLeaf {
    path: PathBuf,
    held: File,
}
impl SocketLeaf {
    pub(super) fn from_listener(
        path: &Path,
        listener: &std::os::unix::net::UnixListener,
    ) -> Result<Self> {
        let held = File::from(rustix::fs::open(
            path,
            rustix::fs::OFlags::PATH | rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        require(
            held.metadata()?.file_type().is_socket() && socket_identity::matches(listener, &held)?,
            "prebound runtime listener/VFS identity changed; preserved",
        )?;
        let owner = Self {
            path: path.into(),
            held,
        };
        require(
            owner.matches()?,
            "prebound runtime path replaced during authentication; preserved",
        )?;
        Ok(owner)
    }
    #[cfg(test)]
    pub(super) fn claim(path: &Path) -> Result<Self> {
        let held = File::from(rustix::fs::open(
            path,
            rustix::fs::OFlags::PATH | rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        require(
            held.metadata()?.file_type().is_socket(),
            "runtime leaf is not a socket; preserved",
        )?;
        let process = rustix::process::pidfd_open(
            rustix::process::getpid(),
            rustix::process::PidfdFlags::empty(),
        )?;
        // This dedicated CLI owns the ZMQ context. Duplicating an actual local
        // socket FD avoids inferring listener ownership from a pathname alone.
        for entry in fs::read_dir("/proc/self/fd")? {
            let entry = entry?;
            let Ok(target) = fs::read_link(entry.path()) else {
                continue;
            };
            let Some(inode) = target
                .to_str()
                .and_then(|s| s.strip_prefix("socket:["))
                .and_then(|s| s.strip_suffix(']'))
                .and_then(|s| s.parse::<u64>().ok())
            else {
                continue;
            };
            let Some(number) = entry
                .file_name()
                .to_str()
                .and_then(|s| s.parse::<i32>().ok())
            else {
                continue;
            };
            let duplicate = match rustix::process::pidfd_getfd(
                &process,
                number,
                rustix::process::PidfdGetfdFlags::empty(),
            ) {
                Ok(fd) => fd,
                Err(rustix::io::Errno::BADF) => continue,
                Err(error) => {
                    return Err(format!(
                        "runtime listener duplication denied; shared leaf preserved: {error}"
                    )
                    .into())
                }
            };
            if rustix::fs::fstat(&duplicate)?.st_ino != inode {
                continue;
            }
            if socket_identity::matches(&duplicate, &held)? {
                let owner = Self {
                    path: path.into(),
                    held,
                };
                require(
                    owner.matches()?,
                    "runtime leaf replaced during kernel authentication; preserved",
                )?;
                return Ok(owner);
            }
        }
        Err("runtime listener/VFS binding unproved; shared leaf preserved".into())
    }
    fn matches(&self) -> Result<bool> {
        let held = self.held.metadata()?;
        match fs::symlink_metadata(&self.path) {
            Ok(current) => Ok((held.dev(), held.ino()) == (current.dev(), current.ino())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
}
impl Drop for SocketLeaf {
    fn drop(&mut self) {
        if self.matches().unwrap_or(false) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::{
        fs::symlink,
        net::{UnixDatagram, UnixListener},
    };
    #[test]
    fn no_listener_regular_and_broken_alias_nodes_are_preserved() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("leaf");
        fs::write(&path, b"foreign").unwrap();
        assert!(SocketLeaf::claim(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"foreign");
        fs::remove_file(&path).unwrap();
        symlink("missing", &path).unwrap();
        assert!(SocketLeaf::claim(&path).is_err());
        assert_eq!(fs::read_link(&path).unwrap(), Path::new("missing"));
    }
    #[test]
    fn replacement_before_initial_claim_never_becomes_cleanup_authority() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("leaf");
        let _listener = UnixListener::bind(&path).unwrap();
        fs::remove_file(&path).unwrap();
        let foreign = UnixDatagram::bind(&path).unwrap();
        let identity = fs::symlink_metadata(&path).unwrap().ino();
        drop(foreign);
        assert!(SocketLeaf::claim(&path).is_err());
        assert_eq!(fs::symlink_metadata(&path).unwrap().ino(), identity);
    }
    #[test]
    fn verified_owner_removes_original_and_preserves_replacement() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("leaf");
        let listener = UnixListener::bind(&path).unwrap();
        let owner = SocketLeaf::claim(&path).unwrap();
        drop(listener);
        drop(owner);
        assert!(!path.exists());
        let _listener = UnixListener::bind(&path).unwrap();
        let owner = SocketLeaf::claim(&path).unwrap();
        fs::remove_file(&path).unwrap();
        fs::write(&path, b"foreign").unwrap();
        drop(owner);
        assert_eq!(fs::read(&path).unwrap(), b"foreign");
    }
}
