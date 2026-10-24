//! Mandatory original inputs and bounded before/after executable identities.
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
        "openpilot/cereal/messaging/__init__.py",
        "1938c80c71dba0855700015bc03371786a711c67bfe3a0c5e073ed790d5e10a9",
    ),
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
    (
        "openpilot/common/api.py",
        "10c6b0673654a798d7d32591dc5b67f39650aed086b812aff8ff62ae64da9177",
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
        "openpilot/system/athena/registration.py",
        "461b5976d0e0ad7849c8096e5372dc3514da24fcd7c1924c97027cdcb62a0924",
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
        "openpilot/system/hardware/tici/hardware.py",
        "cf51658b4f19afdb61de24fe83ef85949c46c8265918dcf07d6b4c4dfb98cabd",
    ),
    (
        "openpilot/system/logmessaged.py",
        "93d94801185350ef4470bd482d37ed7f6d466e9b100ad10aa16fa6e8570fb02a",
    ),
    (
        "openpilot/system/version.py",
        "69b91ce6fc8fe3b164c4f6c23e9514c89a4b6759043cfb607b733c7e006144d7",
    ),
    (
        "rust/tools/check_registration.py",
        "6913c9666fc8fd3f04081ea357792f41adfb75d15e23769c9a7dddc1b18fe360",
    ),
    (
        "rust/tools/logmessaged_native.py",
        "3366e64eb36b0bad9b10023c46217f61c66aae0fe823120ae23d597d2456c872",
    ),
    (
        "rust/tools/logmessaged_reference.py",
        "ce6d6764911daaab1e9e5ce70152d9f5efc9b5ea006482064bb8192d36f735e3",
    ),
    (
        "rust/tools/original_params_binding.py",
        "b5fa10da96aecca63bafef0c03f6ddf9fe5fde89683f10e70742b4e919c38457",
    ),
    (
        "rust/tools/registration_collector_delay.c",
        "f9880c8a3c1efff1b28ec97f8f98b4f432e19323918f4c91e889b43844d384a1",
    ),
    (
        "rust/tools/registration_source.py",
        "d419816f209f86b8395ab61c262c0ff7b839c4b63b944809a6f4c8ebd5721c31",
    ),
];
fn retired(name: &str) -> bool {
    matches!(
        name,
        "rust/tools/check_registration.py" | "rust/tools/registration_source.py"
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
            return Err(format!("registration source changed: {name}").into());
        }
        values.push((name.into(), JsonValue::text(expected)));
    }
    v::object(values)
}
fn regular(path: &Path) -> Result<File> {
    let file = File::from(rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?);
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "registration executable is not a regular file",
        )
        .into());
    }
    Ok(file)
}
pub(super) struct Artifact {
    _file: File,
    device: u64,
    inode: u64,
    pub(super) sha256: String,
}
impl Artifact {
    pub(super) fn open(path: &Path) -> Result<Self> {
        let mut file = regular(path)?;
        let metadata = file.metadata()?;
        let mut digest = Sha256::new();
        let mut buffer = [0; 65536];
        loop {
            let size = file.read(&mut buffer)?;
            if size == 0 {
                break;
            }
            digest.update(&buffer[..size]);
        }
        Ok(Self {
            _file: file,
            device: metadata.dev(),
            inode: metadata.ino(),
            sha256: format!("{:x}", digest.finalize()),
        })
    }
    pub(super) fn verify(&self, path: &Path) -> Result<()> {
        let current = Self::open(path)?;
        if (self.device, self.inode, &self.sha256)
            != (current.device, current.inode, &current.sha256)
        {
            return Err(format!(
                "registration executable changed during execution: {}",
                path.display()
            )
            .into());
        }
        Ok(())
    }
}
