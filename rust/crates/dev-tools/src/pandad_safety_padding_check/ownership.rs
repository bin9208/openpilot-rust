use crate::seal::Result;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, Metadata},
    io::{Read, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};
const PINS: &[(&str, &str)] = &[
    (
        "rust/tools/check_pandad_safety.py",
        "56a08236f486458df4398eb97e85ae11cbebcdb3c04e75f6ca0b4360cac1c9bc",
    ),
    (
        "rust/tools/pandad_safety_source.cc",
        "9f99911342e1b28243a347b8d82708a48faacd2c35e1f753a36099980941d8f0",
    ),
    (
        "openpilot/selfdrive/pandad/panda_safety.cc",
        "f5a583f5c574c9a2bbf9e7940c6c9529a89bcd5d2f135071d3bb934827169717",
    ),
    (
        "openpilot/selfdrive/pandad/pandad.h",
        "b13bd975d585eb6ce29112a924fcfe554991dcf343e2fe9a2be412452ba71a47",
    ),
    (
        "openpilot/common/params.cc",
        "2947645ca30b57d23feeace0e88bead5709c1681dc21359fb1e5a225ca63b187",
    ),
    (
        "openpilot/common/params.h",
        "47d08fa1987c9150e55d09a2a533a0a115851aadb34bd1a0c82325e0be8726b3",
    ),
];
const RETIRED_CHECKER: &str = "rust/tools/check_pandad_safety_padding.py";
const RETIRED_SHA256: &str = "f0d0867a9481a698df8da04d472bdf2e2d6aa77da2a3ae29fba08aae74d5a836";
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
    path: PathBuf,
    file: File,
    metadata: Metadata,
    pub(super) hash: String,
}
impl Bound {
    pub(super) fn open(path: &Path) -> Result<Self> {
        let fd = rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        let mut file = File::from(fd);
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(format!("not a regular safety-padding input: {}", path.display()).into());
        }
        let mut digest = Sha256::new();
        let mut bytes = [0; 65536];
        loop {
            let size = file.read(&mut bytes)?;
            if size == 0 {
                break;
            }
            digest.update(&bytes[..size]);
        }
        if identity(&metadata) != identity(&file.metadata()?) {
            return Err("safety-padding input changed during hashing".into());
        }
        Ok(Self {
            path: path.into(),
            file,
            metadata,
            hash: format!("{:x}", digest.finalize()),
        })
    }
    pub(super) fn verify(&self) -> Result<()> {
        let current = Self::open(&self.path)?;
        if identity(&self.metadata) != identity(&self.file.metadata()?)
            || identity(&self.metadata) != identity(&current.metadata)
            || self.hash != current.hash
        {
            return Err(format!("safety-padding input changed: {}", self.path.display()).into());
        }
        Ok(())
    }
    pub(super) fn relative_to(&self, root: &Path) -> String {
        self.path
            .strip_prefix(root)
            .expect("admitted source root")
            .to_str()
            .expect("fixed ASCII source name")
            .into()
    }
}
pub(super) fn sources(root: &Path) -> Result<Vec<Bound>> {
    let mut pins = PINS
        .iter()
        .map(|(name, expected)| {
            let bound = Bound::open(&root.join(name))?;
            if bound.hash != *expected {
                return Err(format!("original source changed: {name}").into());
            }
            Ok(bound)
        })
        .collect::<Result<Vec<_>>>()?;
    if let Some(bound) = historical(root)? {
        pins.push(bound);
    }
    Ok(pins)
}
fn historical(root: &Path) -> Result<Option<Bound>> {
    let parent = root.join("rust/tools");
    if !fs::metadata(&parent)?.is_dir() {
        return Err("safety-padding source tools parent is not a directory".into());
    }
    let path = root.join(RETIRED_CHECKER);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
        Ok(metadata) if metadata.file_type().is_file() => {
            let bound = Bound::open(&path)?;
            if bound.hash != RETIRED_SHA256 || !fs::symlink_metadata(&path)?.file_type().is_file() {
                return Err("historical safety-padding checker changed".into());
            }
            Ok(Some(bound))
        }
        Ok(_) => Err("historical safety-padding checker is not a regular file".into()),
    }
}
pub(super) fn verify_sources(root: &Path, pins: &[Bound]) -> Result<()> {
    let was_present = pins
        .iter()
        .any(|pin| pin.path == root.join(RETIRED_CHECKER));
    let parent = root.join("rust/tools");
    if !fs::metadata(&parent)?.is_dir() {
        return Err("safety-padding source tools parent is not a directory".into());
    }
    let now_present = match fs::symlink_metadata(root.join(RETIRED_CHECKER)) {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    if was_present != now_present {
        return Err("historical safety-padding checker presence changed".into());
    }
    let now = historical(root)?;
    if was_present != now.is_some() {
        return Err("historical safety-padding checker presence changed".into());
    }
    for pin in pins {
        pin.verify()?;
    }
    Ok(())
}
pub(super) struct Publication {
    path: PathBuf,
    file: File,
}
impl Publication {
    pub(super) fn remove_owned(&self) {
        let Ok(owned) = self.file.metadata() else {
            return;
        };
        if fs::symlink_metadata(&self.path)
            .is_ok_and(|m| (m.dev(), m.ino()) == (owned.dev(), owned.ino()))
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}
pub(super) fn publish(path: &Path, value: &Value) -> Result<Publication> {
    let mut temporary =
        tempfile::NamedTempFile::new_in(path.parent().ok_or("receipt parent missing")?)?;
    serde_json::to_writer_pretty(temporary.as_file_mut(), value)?;
    temporary.write_all(b"\n")?;
    temporary.flush()?;
    let file = temporary.persist_noclobber(path).map_err(|e| e.error)?;
    Ok(Publication {
        path: path.into(),
        file,
    })
}
