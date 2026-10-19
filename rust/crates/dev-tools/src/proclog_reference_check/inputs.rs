//! Retained regular-file identities and relevant original/schema seals only.
use super::Result;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, Metadata},
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};
const PINS: [(&str, &str); 8] = [
    (
        "rust/tools/check_proclog_reference.py",
        "a84ddb66a4bb670b29fd814ad4b950825231ef63cac09f87856e293da398bc47",
    ),
    (
        "openpilot/system/proclogd.py",
        "dfa0ed0970eceab7896addb3befa5ef1c398559c88720891d2d1c539703a1248",
    ),
    (
        "openpilot/cereal/services.py",
        "a0745e2ef00036f0eef08b453a4798b4281549ea9ac6dc91c8b348ef76131edb",
    ),
    (
        "openpilot/cereal/log.capnp",
        "fdecc98871c0ebc11417a558a600de6ff96cc51c2ce620122d39b2cee55f1620",
    ),
    (
        "openpilot/cereal/custom.capnp",
        "102d0aeb45effe3d15abb02a8d930834f8355f9817bf6e135a98435d9a832693",
    ),
    (
        "openpilot/cereal/deprecated.capnp",
        "65079ff035a29c4c267d07ef7240c316c7eaa81b52f795142ed7603489a61355",
    ),
    (
        "opendbc_repo/opendbc/car/car.capnp",
        "6a4d28799933e7728a0a3d4d5a4d47c64fb11e9a6d75e3d4494d2344aefebfda",
    ),
    (
        "openpilot/cereal/include/c++.capnp",
        "fb306076cd38c27af1aed20fac6395e9a46fbe5b5df6a248b5f1b6845a079c44",
    ),
];
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
pub struct Bound {
    file: File,
    metadata: Metadata,
    path: PathBuf,
}
impl Bound {
    pub fn open(path: &Path, pin: Option<&str>) -> Result<Self> {
        let fd = rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        let mut file = File::from(fd);
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err("procLog checker input is not a regular file".into());
        }
        if let Some(pin) = pin {
            let mut data = vec![];
            file.read_to_end(&mut data)?;
            if format!("{:x}", Sha256::digest(&data)) != pin {
                return Err(format!("procLog original source changed: {}", path.display()).into());
            }
        }
        let bound = Self {
            file,
            metadata,
            path: path.to_owned(),
        };
        bound.verify_path(path)?;
        Ok(bound)
    }
    #[cfg(feature = "aarch64-check-native")]
    pub(crate) fn aliases(&self, path: &Path) -> Result<bool> {
        match fs::metadata(path) {
            Ok(metadata) => {
                Ok((metadata.dev(), metadata.ino()) == (self.metadata.dev(), self.metadata.ino()))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
    pub fn verify_path(&self, path: &Path) -> Result<()> {
        if identity(&self.metadata) != identity(&self.file.metadata()?)
            || identity(&self.metadata) != identity(&fs::metadata(path)?)
        {
            return Err(format!("procLog checker input changed: {}", path.display()).into());
        }
        Ok(())
    }
}
pub struct Sources {
    root: PathBuf,
    files: Vec<Bound>,
    helper_present: bool,
}
impl Sources {
    pub fn open(root: &Path) -> Result<Self> {
        let mut files = vec![];
        let helper_present = match fs::symlink_metadata(root.join(PINS[0].0)) {
            Ok(_) => true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => return Err(e.into()),
        };
        for (index, (name, pin)) in PINS.iter().enumerate() {
            if index != 0 || helper_present {
                files.push(Bound::open(&root.join(name), Some(pin))?);
            }
        }
        Ok(Self {
            root: root.to_owned(),
            files,
            helper_present,
        })
    }
    #[cfg(feature = "aarch64-check-native")]
    pub(crate) fn aliases(&self, path: &Path) -> Result<bool> {
        for file in &self.files {
            if file.aliases(path)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub fn verify(&self) -> Result<()> {
        for file in &self.files {
            file.verify_path(&file.path)?;
        }
        let current = Self::open(&self.root)?;
        if current.helper_present != self.helper_present {
            return Err("procLog source helper roster changed".into());
        }
        Ok(())
    }
}
