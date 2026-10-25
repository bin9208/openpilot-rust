use super::require;
use crate::seal::Result;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{self, Read},
    os::unix::fs::MetadataExt,
    path::Path,
};
pub(super) const RETIRED: &str = "rust/tools/check_soundd_daemon.py";
pub(super) const ORIGINAL: &str =
    "fadc1ad42b382d4e4a6be128ead810e2b694e69b937525cb280e8f59cd799664";
const PINS: &[(&str, &str)] = &[
    (
        "msgq_repo/msgq/__init__.py",
        "9b9e5e26cf4190e9aaf056cd90771fb95228f665d1171a9ee919b432a0d2d368",
    ),
    (
        "msgq_repo/msgq/event.cc",
        "e76de4967fe3845867917b72d770dd24847d963d6d3d460118c4b7453a192fea",
    ),
    (
        "msgq_repo/msgq/impl_fake.cc",
        "e3268c6103187675acc844c1506ca30f648bc44b0591739a94b682962f01a286",
    ),
    (
        "msgq_repo/msgq/impl_fake.h",
        "8fd5c8c27c03e29793a396cb04d59f98ae4f731e20402b6d853acc721f0d00d2",
    ),
    (
        "msgq_repo/msgq/impl_msgq.cc",
        "3201986c605f3657597bda63b33c30a16f4d597cd8588d5e09f620da6ab93c29",
    ),
    (
        "msgq_repo/msgq/ipc.cc",
        "096b7a00e03efc9cdd7758351906edb639e23eaf8879fa4ca8026e15f965b544",
    ),
    (
        "msgq_repo/msgq/ipc.h",
        "1572352e5b24014bc1e26410efd7be9ff27a6b1c577b6614a9b74e7d6d313337",
    ),
    (
        "msgq_repo/msgq/ipc.pxd",
        "010c68272ced25786736d34b1a8833c3054add34fab1d042feb66dca77edccaf",
    ),
    (
        "msgq_repo/msgq/ipc_pyx.pyx",
        "02cf21d0232967eed7bd68ed0209e55f5f1e3edef3675671bc6524ca64976fa4",
    ),
    (
        "msgq_repo/msgq/msgq.cc",
        "5910ffb3bc2a4f023b4bbc612e41b76ddeaace2eb84ae8b127e8736905d7a6c2",
    ),
    (
        "msgq_repo/msgq/visionipc/__init__.py",
        "ddbaacc6331b8072e78b17b8abb659cca13f8c3cd2dde4f1e0340b2934be01aa",
    ),
    (
        "msgq_repo/msgq/visionipc/visionipc.pxd",
        "770ef106d0bcf43e735691bc669d2f8ca0b0dd6e3054dc2d39f54834975391ca",
    ),
    (
        "msgq_repo/msgq/visionipc/visionipc_pyx.pxd",
        "346f11eb56f334fd261bbc332e54c97636e35f85236d5d3a943f663e365756c0",
    ),
    (
        "msgq_repo/msgq/visionipc/visionipc_pyx.pyx",
        "37ca79aff2efa6dddda2a4a64e62af8907805d1708c7c089acdcb0089789dfd7",
    ),
    (
        "opendbc_repo/opendbc/car/car.capnp",
        "6a4d28799933e7728a0a3d4d5a4d47c64fb11e9a6d75e3d4494d2344aefebfda",
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
        "openpilot/cereal/log.capnp",
        "fdecc98871c0ebc11417a558a600de6ff96cc51c2ce620122d39b2cee55f1620",
    ),
    (
        "openpilot/cereal/services.py",
        "a0745e2ef00036f0eef08b453a4798b4281549ea9ac6dc91c8b348ef76131edb",
    ),
    (RETIRED, ORIGINAL),
    (
        "rust/tools/fixtures/soundd/portaudio.c",
        "ae2da5981c0ed5e898b1db1afa20d9e99101369fe48e39f2260562fe2b7c5eb2",
    ),
];
fn regular(path: &Path) -> Result<File> {
    let file = File::from(rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?);
    require(
        file.metadata()?.is_file(),
        format!("not a regular input: {}", path.display()),
    )?;
    Ok(file)
}
pub fn validate_source(root: &Path) -> Result<Value> {
    let mut live = serde_json::Map::new();
    for &(name, expected) in PINS {
        let path = root.join(name);
        if name == RETIRED {
            match fs::symlink_metadata(&path) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Ok(_) => return Err("retired soundd Python checker reappeared".into()),
                Err(error) => return Err(error.into()),
            }
        }
        fs::symlink_metadata(&path)?;
        let mut raw = Vec::new();
        regular(&path)?.read_to_end(&mut raw)?;
        require(
            format!("{:x}", Sha256::digest(&raw)) == expected,
            format!("soundd checker original source changed: {name}"),
        )?;
        live.insert(name.into(), json!(expected));
    }
    Ok(Value::Object(live))
}
/// Retain an opened regular descriptor through execution; compare before/after
/// path identity and content. This is bounded replacement detection, not an ABA claim.
pub(super) struct Artifact {
    _file: File,
    dev: u64,
    ino: u64,
    pub(super) sha256: String,
}
impl Artifact {
    pub(super) fn open(path: &Path) -> Result<Self> {
        let mut file = regular(path)?;
        let m = file.metadata()?;
        let mut hash = Sha256::new();
        let mut bytes = [0; 65536];
        loop {
            let n = file.read(&mut bytes)?;
            if n == 0 {
                break;
            }
            hash.update(&bytes[..n]);
        }
        Ok(Self {
            _file: file,
            dev: m.dev(),
            ino: m.ino(),
            sha256: format!("{:x}", hash.finalize()),
        })
    }
    pub(super) fn verify(&self, path: &Path) -> Result<()> {
        let b = Self::open(path)?;
        require(
            (self.dev, self.ino, &self.sha256) == (b.dev, b.ino, &b.sha256),
            format!(
                "executed input changed during soundd check: {}",
                path.display()
            ),
        )
    }
}
