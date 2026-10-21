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
        "rust/tools/check_pandad_protocol.py",
        "927164a1021139642d4e17f2f35d3b531dbafc989161fe871ccc614a7c7e51bf",
    ),
    (
        "rust/tools/pandad_protocol_cases.py",
        "3eac4a39401de9311c5a5ea5211afebac1deede3a055f9d15fdb45d7576ee5bf",
    ),
    (
        "rust/tools/pandad_protocol_source.cc",
        "b1409527acd09687c9fea57f01f24fb6af0daff46fe89dc065d7704ddf58c011",
    ),
    (
        "openpilot/selfdrive/pandad/panda.cc",
        "fb50fce93675ae93403b478a6bc32621af10359420281a23dc16b3e283f98089",
    ),
    (
        "openpilot/selfdrive/pandad/panda.h",
        "faebd7a679a0fc8b44ca6b42fe0c6ed9aeddb52f3d4980db5dc025e05dbdc15a",
    ),
    (
        "openpilot/selfdrive/pandad/spi_alert.h",
        "40b34417b4c6c19185f2f3f7f03a0b34e7a4127953b587358d0c841d052d5063",
    ),
];
// The corpus module remains owned by the still-active CAN-I/O Python caller.
const RETIRED: [&str; 1] = ["rust/tools/check_pandad_protocol.py"];
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
            return Err("Panda protocol input is not a regular file".into());
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
            return Err("Panda protocol input changed during execution".into());
        }
        Ok(())
    }
}
fn retired_absent(root: &Path) -> Result<bool> {
    match fs::symlink_metadata(root.join(RETIRED[0])) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(false),
        Ok(_) => Err("Panda protocol historical checker is not regular".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error.into()),
    }
}
pub(super) struct Sources {
    root: PathBuf,
    absent: bool,
    entries: Vec<(&'static str, Bound)>,
}
impl Sources {
    pub fn iter(&self) -> impl Iterator<Item = &Bound> {
        self.entries.iter().map(|(_, bound)| bound)
    }
    pub fn hashes(&self) -> serde_json::Map<String, Value> {
        self.entries
            .iter()
            .map(|(name, bound)| ((*name).into(), Value::String(bound.hash.clone())))
            .collect()
    }
    pub fn absent(&self) -> bool {
        self.absent
    }
    pub fn verify(&self) -> Result<()> {
        if retired_absent(&self.root)? != self.absent {
            return Err(
                "Panda protocol historical source presence changed during execution".into(),
            );
        }
        for bound in self.iter() {
            bound.verify()?;
        }
        Ok(())
    }
}
pub(super) fn sources(root: &Path) -> Result<Sources> {
    let absent = retired_absent(root)?;
    let mut entries = Vec::new();
    for &(name, hash) in PINS {
        if absent && RETIRED.contains(&name) {
            continue;
        }
        let bound = Bound::open(&root.join(name))?;
        if bound.hash != hash {
            return Err(format!("Panda protocol source changed: {name}").into());
        }
        entries.push((name, bound));
    }
    Ok(Sources {
        root: root.into(),
        absent,
        entries,
    })
}
pub(super) struct Publication {
    path: PathBuf,
    file: File,
    committed: bool,
}
impl Publication {
    pub fn write<T: serde::Serialize + ?Sized>(path: &Path, value: &T) -> Result<Self> {
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
            return Err("Panda protocol report replaced before commit".into());
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
