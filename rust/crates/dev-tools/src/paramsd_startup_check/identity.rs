use crate::seal::Result;
use rustix::fs::{open, Mode, OFlags};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, Metadata},
    io::{Read, Seek, SeekFrom},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

fn identity(m: &Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
    (
        m.dev(),
        m.ino(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}

pub(super) struct Bound {
    pub path: PathBuf,
    file: File,
    metadata: Metadata,
    pub hash: String,
}
impl Bound {
    pub fn open(path: &Path, expected: Option<&str>) -> Result<Self> {
        let mut file = File::from(open(
            path,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NONBLOCK | OFlags::NOFOLLOW,
            Mode::empty(),
        )?);
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(format!("not a regular startup input: {}", path.display()).into());
        }
        let mut hasher = Sha256::new();
        let mut chunk = [0; 65536];
        loop {
            let count = file.read(&mut chunk)?;
            if count == 0 {
                break;
            }
            hasher.update(&chunk[..count]);
        }
        let hash = format!("{:x}", hasher.finalize());
        if expected.is_some_and(|expected| expected != hash)
            || identity(&metadata) != identity(&file.metadata()?)
        {
            return Err(format!("startup input changed: {}", path.display()).into());
        }
        Ok(Self {
            path: path.into(),
            file,
            metadata,
            hash,
        })
    }
    pub fn verify(&self) -> Result<()> {
        let current = Self::open(&self.path, Some(&self.hash))?;
        if identity(&self.metadata) != identity(&current.metadata)
            || identity(&self.metadata) != identity(&self.file.metadata()?)
        {
            return Err(format!("startup input identity changed: {}", self.path.display()).into());
        }
        Ok(())
    }
    pub fn native_elf(&mut self) -> Result<()> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut header = [0; 64];
        self.file.read_exact(&mut header)?;
        let machine = match std::env::consts::ARCH {
            "x86_64" => 62,
            "aarch64" => 183,
            _ => return Err("unsupported native startup host".into()),
        };
        if &header[..7] != b"\x7fELF\x02\x01\x01"
            || !matches!(u16::from_le_bytes([header[16], header[17]]), 2 | 3)
            || u16::from_le_bytes([header[18], header[19]]) != machine
            || self.metadata.mode() & 0o111 == 0
        {
            return Err("startup binary is not an executable native host ELF".into());
        }
        self.verify()
    }
}

pub(super) fn absent(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error.into()),
        Ok(_) => Ok(false),
    }
}

pub(super) struct Directory {
    path: PathBuf,
    dev: u64,
    ino: u64,
}
impl Directory {
    pub fn create(path: &Path) -> Result<Self> {
        fs::create_dir(path)?;
        let m = fs::symlink_metadata(path)?;
        Ok(Self {
            path: path.into(),
            dev: m.dev(),
            ino: m.ino(),
        })
    }
    pub fn verify(&self) -> Result<()> {
        let m = fs::symlink_metadata(&self.path)?;
        if !m.is_dir() || (m.dev(), m.ino()) != (self.dev, self.ino) {
            return Err("startup evidence directory identity changed".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn bounds_reject_replacement_mutation_symlink_and_directory() -> Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("held");
        fs::write(&path, b"original")?;
        let held = Bound::open(&path, None)?;
        held.verify()?;
        fs::rename(&path, root.path().join("old"))?;
        fs::write(&path, b"original")?;
        assert!(held.verify().is_err(), "same-byte replacement admitted");
        let changed = Bound::open(&path, None)?;
        fs::write(&path, b"different")?;
        assert!(changed.verify().is_err());
        let link = root.path().join("link");
        symlink(&path, &link)?;
        assert!(Bound::open(&link, None).is_err());
        assert!(Bound::open(root.path(), None).is_err());
        Ok(())
    }

    #[test]
    fn absent_checks_include_dangling_symlinks() -> Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("historical");
        assert!(absent(&path)?);
        symlink(root.path().join("does-not-exist"), &path)?;
        assert!(!absent(&path)?);
        Ok(())
    }
}
