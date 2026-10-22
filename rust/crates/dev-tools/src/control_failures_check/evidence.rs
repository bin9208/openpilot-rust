//! Atomic exclusive claim; all writes stay anchored to the held owned inode.
use crate::seal::Result;
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::{fd::AsRawFd, unix::fs::MetadataExt},
    path::{Path, PathBuf},
};
pub(super) struct Evidence {
    path: PathBuf,
    dir: File,
}
pub(super) struct Inherited<'a>(&'a File, rustix::io::FdFlags);
impl Drop for Inherited<'_> {
    fn drop(&mut self) {
        let _ = rustix::io::fcntl_setfd(self.0, self.1);
    }
}
impl Evidence {
    pub fn claim(path: &Path) -> Result<Self> {
        let leaf = path.file_name().ok_or("evidence leaf required")?;
        if leaf == "." || leaf == ".." {
            return Err("evidence leaf invalid".into());
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let parent = parent.canonicalize()?;
        let path = parent.join(leaf);
        match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) => return Err("evidence path already occupied".into()),
            Err(e) => return Err(e.into()),
        }
        let temporary = tempfile::Builder::new()
            .prefix(".control-failures-")
            .tempdir_in(&parent)?;
        let dir = File::open(temporary.path())?;
        let temporary = temporary.keep();
        if let Err(error) = rustix::fs::renameat_with(
            rustix::fs::CWD,
            &temporary,
            rustix::fs::CWD,
            &path,
            rustix::fs::RenameFlags::NOREPLACE,
        ) {
            let own = dir.metadata()?;
            if fs::symlink_metadata(&temporary)
                .is_ok_and(|m| m.is_dir() && (m.dev(), m.ino()) == (own.dev(), own.ino()))
            {
                let _ = fs::remove_dir(&temporary);
            }
            return Err(error.into());
        }
        let out = Self { path, dir };
        out.verify()?;
        Ok(out)
    }
    pub fn anchor(&self) -> PathBuf {
        PathBuf::from(format!("/proc/self/fd/{}", self.dir.as_raw_fd()))
    }
    pub fn verify(&self) -> Result<()> {
        let old = self.dir.metadata()?;
        let current = fs::symlink_metadata(&self.path)?;
        if !current.is_dir() || (old.dev(), old.ino()) != (current.dev(), current.ino()) {
            return Err("owned evidence path replaced".into());
        }
        Ok(())
    }
    pub fn stage_case(&self) -> Result<PendingCase<'_>> {
        self.verify()?;
        let temporary = tempfile::Builder::new()
            .prefix(".case-")
            .tempdir_in(self.anchor())?;
        let dir = File::from(rustix::fs::open(
            temporary.path(),
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        let path = temporary.keep();
        let name = path
            .file_name()
            .ok_or("private case leaf missing")?
            .to_owned();
        Ok(PendingCase {
            root: self,
            dir,
            name,
        })
    }
    pub fn case(&self, name: &str) -> Result<Case<'_>> {
        self.stage_case()?.publish(name)
    }
    pub fn bytes(path: &Path, bytes: &[u8]) -> Result<()> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    }
    pub fn json(path: &Path, value: &Value) -> Result<()> {
        let mut bytes = serde_json::to_vec_pretty(value)?;
        bytes.push(b'\n');
        Self::bytes(path, &bytes)
    }
    pub fn report(&self, value: &Value) -> Result<()> {
        let temporary = self.anchor().join(".results.pending");
        Self::json(&temporary, value)?;
        self.verify()?;
        rustix::fs::renameat_with(
            rustix::fs::CWD,
            &temporary,
            rustix::fs::CWD,
            self.anchor().join("results.json"),
            rustix::fs::RenameFlags::NOREPLACE,
        )?;
        self.dir.sync_all()?;
        Ok(())
    }
}

/// Retain the case inode too: ROOT_FD/CASE alone still traverses replaceable CASE.
pub(super) struct Case<'a> {
    root: &'a Evidence,
    name: String,
    dir: File,
}
impl Case<'_> {
    pub fn anchor(&self) -> PathBuf {
        PathBuf::from(format!("/proc/self/fd/{}", self.dir.as_raw_fd()))
    }
    pub fn verify(&self) -> Result<()> {
        self.root.verify()?;
        let own = self.dir.metadata()?;
        let current = fs::symlink_metadata(self.root.anchor().join(&self.name))?;
        if !current.is_dir() || (own.dev(), own.ino()) != (current.dev(), current.ino()) {
            return Err("owned case path replaced".into());
        }
        Ok(())
    }
    pub fn inherit(&self) -> Result<Inherited<'_>> {
        let flags = rustix::io::fcntl_getfd(&self.dir)?;
        rustix::io::fcntl_setfd(&self.dir, flags & !rustix::io::FdFlags::CLOEXEC)?;
        Ok(Inherited(&self.dir, flags))
    }
}

/// A held private inode, never an opened replacement of the public CASE name.
/// Failed publication leaves this empty owned stage in evidence for diagnosis;
/// no check-then-unlink cleanup can remove a foreign replacement.
pub(super) struct PendingCase<'a> {
    root: &'a Evidence,
    dir: File,
    name: std::ffi::OsString,
}
impl<'a> PendingCase<'a> {
    pub fn publish(self, name: &str) -> Result<Case<'a>> {
        self.root.verify()?;
        rustix::fs::renameat_with(
            &self.root.dir,
            &self.name,
            &self.root.dir,
            name,
            rustix::fs::RenameFlags::NOREPLACE,
        )?;
        let case = Case {
            root: self.root,
            name: name.to_owned(),
            dir: self.dir,
        };
        case.verify()?;
        Ok(case)
    }
}
