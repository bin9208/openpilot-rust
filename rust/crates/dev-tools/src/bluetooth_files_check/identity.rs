use crate::seal::Result;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, Metadata},
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};
fn id(m: &Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
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
pub(super) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(super) struct Bound {
    pub path: PathBuf,
    file: File,
    metadata: Metadata,
    pub hash: String,
}
impl Bound {
    pub fn open(path: &Path) -> Result<Self> {
        let fd = rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        let mut file = File::from(fd);
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err("Bluetooth files input is not a regular file".into());
        }
        let mut hasher = Sha256::new();
        let mut bytes = [0; 65536];
        loop {
            let n = file.read(&mut bytes)?;
            if n == 0 {
                break;
            }
            hasher.update(&bytes[..n]);
        }
        let bound = Self {
            path: path.into(),
            file,
            metadata,
            hash: format!("{:x}", hasher.finalize()),
        };
        bound.verify()?;
        Ok(bound)
    }
    pub fn verify(&self) -> Result<()> {
        if id(&self.metadata) != id(&self.file.metadata()?)
            || id(&self.metadata) != id(&fs::metadata(&self.path)?)
        {
            return Err(format!("Bluetooth files input changed: {}", self.path.display()).into());
        }
        Ok(())
    }
}
