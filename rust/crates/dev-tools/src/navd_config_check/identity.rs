//! Explicit historical source controls and bounded artifact identities.
use super::values as v;
use crate::seal::Result;
use openpilot_log_format::JsonValue;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, Read},
    os::unix::fs::MetadataExt,
    path::Path,
};
const PINS: &[(&str, &str)] = &[
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
        "openpilot/common/api.py",
        "10c6b0673654a798d7d32591dc5b67f39650aed086b812aff8ff62ae64da9177",
    ),
    (
        "openpilot/common/constants.py",
        "fc439d59c24b95ed349b84f1a5862df3ea50e0891dbec4ea3cf5e1e0ccff6513",
    ),
    (
        "openpilot/common/logging_extra.py",
        "ce9b3b3d8498d76e1c4c0352a3e98e72759bb7f367ff8629c1329c46040595c3",
    ),
    (
        "openpilot/common/params.cc",
        "2947645ca30b57d23feeace0e88bead5709c1681dc21359fb1e5a225ca63b187",
    ),
    (
        "openpilot/common/params.h",
        "47d08fa1987c9150e55d09a2a533a0a115851aadb34bd1a0c82325e0be8726b3",
    ),
    (
        "openpilot/common/params.py",
        "4708c127258c0c0de9acd2d670ad9b48d4f3fd050c96deb373ea8cd348afe3a4",
    ),
    (
        "openpilot/common/params_keys.h",
        "b8422e8ad2cd74ec3c5750c3e919f6546519d84fabb5f6ec868b5372713b4d7f",
    ),
    (
        "openpilot/common/params_pyx.pyx",
        "a8cc5e5296dd38a012c55b0867c4563f8e320d25960260d897675dd90f0ea544",
    ),
    (
        "openpilot/common/queue.h",
        "f7c75c847ae63dfd6e864406e722248540a79908270e85ca2674b43407408fcf",
    ),
    (
        "openpilot/common/swaglog.cc",
        "b61e45b8ad799606ba2a78e0b5ee9303a665bad99a67d50f3b7134792bf7ba7c",
    ),
    (
        "openpilot/common/swaglog.h",
        "bcc65e78e40b45366f393fea51286b83ec75792e38c44914f89e3912f5a6841b",
    ),
    (
        "openpilot/common/swaglog.py",
        "d37b623bd9418b72ffd3c4800a1da192c49856cb63bbbc1c09bb24a5ec6f8616",
    ),
    (
        "openpilot/common/timing.h",
        "66a580e240fa4fe4af03bc3073ddd0f4db5d1027720e8adfa92ba448f7c8bc71",
    ),
    (
        "openpilot/common/util.cc",
        "4479ecf72465e8f453d8af78447f7715f02d9397c58a49048f2bbc87a96d6b8a",
    ),
    (
        "openpilot/common/util.h",
        "49db7a80478fb44be3a03ee5224775fa15e62a03f982152eae15f0a4dc5cceea",
    ),
    (
        "openpilot/common/version.h",
        "8d618ada21dcc00618178d67f52c532b95c3eeff0736a9909379a3431e050ba6",
    ),
    (
        "openpilot/selfdrive/navd/helpers.py",
        "1f9ad6d2234f5ed5e0a48846deab45cc6b65c996f816445f81adcd515814e348",
    ),
    (
        "openpilot/selfdrive/navd/navd.py",
        "26280b9bb752de2f75c3dd7e1679e47e238c99d074a1b6f1e858e7faa41aa074",
    ),
    (
        "openpilot/system/hardware/base.h",
        "7ef48c160e8e0e5cba3a8f629ac72904900e41be260aee8e4029b5c1a615e3b9",
    ),
    (
        "openpilot/system/hardware/hw.h",
        "afc36d60417dbaf1683d17f10b32306c1d69f58816a6dad09c8da24484249da7",
    ),
    (
        "openpilot/system/hardware/pc/hardware.h",
        "edee2ab86ca69a3c8f005a823457edfe3a9368703a5583ff0d77b32f8b1e105c",
    ),
    (
        "rust/tools/check_navd_config.py",
        "27c49d708b1cf39682623b6c05877d1efb91fa7ddc5ba65c8ad9b6c81c794629",
    ),
    (
        "rust/tools/check_registration.py",
        "6913c9666fc8fd3f04081ea357792f41adfb75d15e23769c9a7dddc1b18fe360",
    ),
    (
        "rust/tools/navd_config_source.py",
        "0c7b9eb697d972a4ae3b9157daa012872879a1c2df2595c357c01577de2b83c0",
    ),
    (
        "rust/tools/original_params_binding.py",
        "b5fa10da96aecca63bafef0c03f6ddf9fe5fde89683f10e70742b4e919c38457",
    ),
    (
        "rust/tools/registration_source.py",
        "d419816f209f86b8395ab61c262c0ff7b839c4b63b944809a6f4c8ebd5721c31",
    ),
];
fn regular(path: &Path) -> Result<File> {
    let file = File::from(rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?);
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "navd config input is not a regular file",
        )
        .into());
    }
    Ok(file)
}
fn retired(name: &str) -> bool {
    matches!(
        name,
        "rust/tools/check_registration.py"
            | "rust/tools/registration_source.py"
            | "rust/tools/check_navd_config.py"
            | "rust/tools/navd_config_source.py"
    )
}
/// Captured origin identities, independent of whether the helper is still live.
pub(super) fn historical_sources() -> Result<JsonValue> {
    v::object(
        PINS.iter()
            .filter(|(name, _)| retired(name))
            .map(|&(name, sha)| (name.into(), JsonValue::text(sha))),
    )
}
pub(super) fn sources(root: &Path) -> Result<JsonValue> {
    let mut values = Vec::new();
    for &(name, expected) in PINS {
        let path = root.join(name);
        match std::fs::symlink_metadata(&path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound && retired(name) => continue,
            Err(error) => return Err(error.into()),
            Ok(_) => {}
        }
        // A dangling symlink is present: following it must fail, not admit absence.
        let mut input = regular(&path)?;
        let mut bytes = Vec::new();
        input.read_to_end(&mut bytes)?;
        if format!("{:x}", Sha256::digest(&bytes)) != expected {
            return Err(format!("navd config source changed: {name}").into());
        }
        values.push((name.into(), JsonValue::text(expected)));
    }
    v::object(values)
}
pub(super) struct Artifact {
    _file: File,
    dev: u64,
    ino: u64,
    pub(super) sha256: String,
}
impl Artifact {
    pub(super) fn open(path: &Path) -> Result<Self> {
        let mut file = regular(path)?;
        let meta = file.metadata()?;
        let mut hash = Sha256::new();
        let mut buf = [0; 65536];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hash.update(&buf[..n]);
        }
        Ok(Self {
            _file: file,
            dev: meta.dev(),
            ino: meta.ino(),
            sha256: format!("{:x}", hash.finalize()),
        })
    }
    pub(super) fn verify(&self, path: &Path) -> Result<()> {
        let current = Self::open(path)?;
        if (self.dev, self.ino, &self.sha256) != (current.dev, current.ino, &current.sha256) {
            return Err(format!("navd config executable changed: {}", path.display()).into());
        }
        Ok(())
    }
}
