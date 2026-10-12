use super::manifest::{digest, File as Spec};
use crate::Error;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::File,
    path::{Component, Path, PathBuf},
};

pub(super) fn relative(root: &Path, name: &str) -> Result<PathBuf, Error> {
    if name.is_empty()
        || !Path::new(name)
            .components()
            .all(|item| matches!(item, Component::Normal(_)))
    {
        return Err(Error::Contract("local native asset path escapes package"));
    }
    let mut path = root.to_path_buf();
    for component in Path::new(name).components() {
        path.push(component.as_os_str());
        if path.symlink_metadata()?.file_type().is_symlink() {
            return Err(Error::Contract("local native asset is a symlink"));
        }
    }
    Ok(path)
}
fn hash_file(path: &Path) -> Result<String, Error> {
    let mut hash = Sha256::new();
    std::io::copy(&mut File::open(path)?, &mut hash)?;
    Ok(format!("{:x}", hash.finalize()))
}
pub(super) fn verify(root: &Path, files: &[Spec]) -> Result<BTreeSet<String>, Error> {
    let mut paths = BTreeSet::new();
    let mut total = 0u64;
    for file in files {
        total = total
            .checked_add(file.bytes)
            .ok_or(Error::Contract("local native package size overflow"))?;
        if total > 8 << 30 || !digest(&file.sha256) || !paths.insert(file.path.clone()) {
            return Err(Error::Contract("local native package file limits"));
        }
        let path = relative(root, &file.path)?;
        if !path.is_file()
            || path.metadata()?.len() != file.bytes
            || hash_file(&path)? != file.sha256
        {
            return Err(Error::Contract("local native companion checksum mismatch"));
        }
    }
    let required = |name: &str| {
        if !paths.contains(name) {
            return Err(Error::Contract("local native hardware companion missing"));
        }
        relative(root, name)
    };
    required("tinygrad-LICENSE")?;
    crate::probe::validate_descriptor(&std::fs::read(required("probe-gfx1200.json")?)?, "gfx1200")?;
    let catalog = crate::amd_metadata::Catalog::bundled()?;
    for name in [
        "psp_14_0_2_sos.bin",
        "gc_12_0_0_pfp.bin",
        "gc_12_0_0_me.bin",
        "gc_12_0_0_mec.bin",
        "gc_12_0_0_rlc.bin",
        "gc_12_0_0_imu.bin",
        "sdma_7_0_0.bin",
        "smu_14_0_2.bin",
    ] {
        let path = required(&format!("firmware/{name}"))?;
        if catalog.firmware_hashes.get(name) != Some(&hash_file(&path)?) {
            return Err(Error::Contract(
                "local firmware differs from pinned AMD catalog",
            ));
        }
    }
    Ok(paths)
}
