use crate::seal::Result;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

pub(super) const PINS: &[(&str, &str)] = &[
    (
        "rust/tools/check_pandad_device.py",
        "d44e161374b29f91bfb315d9b66eb65acf40e20b64d72b17d5acc3a414cb3c18",
    ),
    (
        "rust/tools/pandad_device_cases.py",
        "33ceb4a8f54680f4aa777e1212a56b6936ead14b03510ba5823a4d0b20cfa78b",
    ),
    (
        "rust/tools/pandad_device_source.cc",
        "96a8b74bf789bf96a91067bf4a174aaca3bbd455133eb4c48bdd8a6598a09197",
    ),
    (
        "openpilot/selfdrive/pandad/panda.cc",
        "fb50fce93675ae93403b478a6bc32621af10359420281a23dc16b3e283f98089",
    ),
    (
        "openpilot/selfdrive/pandad/panda.h",
        "faebd7a679a0fc8b44ca6b42fe0c6ed9aeddb52f3d4980db5dc025e05dbdc15a",
    ),
];
pub(super) struct Bound {
    path: PathBuf,
    file: File,
    pub hash: String,
    dev: u64,
    ino: u64,
}
impl Bound {
    pub fn open(path: &Path) -> Result<Self> {
        let mut file = File::from(rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err("Panda device input is not a regular file".into());
        }
        let mut hash = Sha256::new();
        let mut bytes = [0; 65536];
        loop {
            let count = file.read(&mut bytes)?;
            if count == 0 {
                break;
            }
            hash.update(&bytes[..count]);
        }
        Ok(Self {
            path: path.into(),
            file,
            hash: format!("{:x}", hash.finalize()),
            dev: metadata.dev(),
            ino: metadata.ino(),
        })
    }
    pub fn verify(&self) -> Result<()> {
        let after = Self::open(&self.path)?;
        let held = self.file.metadata()?;
        if (self.dev, self.ino, &self.hash) != (after.dev, after.ino, &after.hash)
            || (held.dev(), held.ino()) != (self.dev, self.ino)
        {
            return Err("Panda device input changed during execution".into());
        }
        Ok(())
    }
}
pub(super) fn sources(root: &Path) -> Result<Vec<Bound>> {
    PINS.iter()
        .map(|(path, hash)| {
            let bound = Bound::open(&root.join(path))?;
            if bound.hash != *hash {
                return Err(format!("Panda device source changed: {path}").into());
            }
            Ok(bound)
        })
        .collect()
}
pub(super) struct Publication {
    path: PathBuf,
    file: File,
    committed: bool,
}
impl Publication {
    pub fn write(path: &Path, value: &Value) -> Result<Self> {
        let mut temporary =
            tempfile::NamedTempFile::new_in(path.parent().ok_or("missing report parent")?)?;
        serde_json::to_writer_pretty(&mut temporary, value)?;
        temporary.write_all(b"\n")?;
        temporary.flush()?;
        let file = temporary.persist_noclobber(path).map_err(|e| e.error)?;
        Ok(Self {
            path: path.into(),
            file,
            committed: false,
        })
    }
    pub fn commit(&mut self) -> Result<()> {
        if !self.owns() {
            return Err("Panda device report replaced before commit".into());
        }
        self.committed = true;
        Ok(())
    }
    fn owns(&self) -> bool {
        let Ok(held) = self.file.metadata() else {
            return false;
        };
        fs::symlink_metadata(&self.path)
            .is_ok_and(|m| m.is_file() && (m.dev(), m.ino()) == (held.dev(), held.ino()))
    }
}
impl Drop for Publication {
    fn drop(&mut self) {
        if !self.committed && self.owns() {
            let _ = fs::remove_file(&self.path);
        }
    }
}
