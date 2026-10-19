//! Success publication never clears caller data on failed admission. A regular
//! report is replaced atomically; cancellation only removes this created inode.
use super::{check, Result};
use std::{
    fs::{self, File},
    io::{self, Write},
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};
pub(super) struct Published {
    file: File,
    path: PathBuf,
    regular: bool,
}
impl Published {
    pub(super) fn write(path: &Path, bytes: &[u8], stop: &AtomicBool) -> Result<Self> {
        let previous = match fs::metadata(path) {
            Ok(m) => Some(m),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        if previous.as_ref().is_some_and(|m| !m.is_file()) {
            // Preserve original streaming output semantics for caller-owned FIFOs and
            // devices, while polling cancellation instead of blocking a FIFO open.
            if previous.as_ref().is_some_and(|m| m.is_dir()) {
                return Err("AArch64 report path is a directory".into());
            }
            let fifo = previous.as_ref().is_some_and(|m| m.file_type().is_fifo());
            let mut file = loop {
                check(stop)?;
                match rustix::fs::open(
                    path,
                    rustix::fs::OFlags::WRONLY
                        | rustix::fs::OFlags::TRUNC
                        | rustix::fs::OFlags::NONBLOCK
                        | rustix::fs::OFlags::CLOEXEC,
                    rustix::fs::Mode::empty(),
                ) {
                    Ok(fd) => break File::from(fd),
                    Err(rustix::io::Errno::NXIO) if fifo => {
                        std::thread::sleep(Duration::from_millis(2))
                    }
                    Err(e) => return Err(e.into()),
                }
            };
            let mut offset = 0;
            while offset < bytes.len() {
                check(stop)?;
                match file.write(&bytes[offset..]) {
                    Ok(0) => return Err("report output closed".into()),
                    Ok(n) => offset += n,
                    Err(e)
                        if matches!(
                            e.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) =>
                    {
                        std::thread::sleep(Duration::from_millis(2))
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            return Ok(Self {
                file,
                path: path.into(),
                regular: false,
            });
        }
        let mut temp =
            tempfile::NamedTempFile::new_in(path.parent().ok_or("report has no parent")?)?;
        if let Some(metadata) = previous {
            temp.as_file().set_permissions(metadata.permissions())?;
        }
        temp.write_all(bytes)?;
        temp.flush()?;
        check(stop)?;
        let file = temp.persist(path).map_err(|e| e.error)?;
        Ok(Self {
            file,
            path: path.into(),
            regular: true,
        })
    }
    pub(super) fn remove_owned(&self) {
        if !self.regular {
            return;
        }
        let Ok(owned) = self.file.metadata() else {
            return;
        };
        let Ok(now) = fs::symlink_metadata(&self.path) else {
            return;
        };
        if now.is_file() && (owned.dev(), owned.ino()) == (now.dev(), now.ino()) {
            let _ = fs::remove_file(&self.path);
        }
    }
}
