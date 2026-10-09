use super::Error;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap, fs::OpenOptions, io::Read, os::unix::fs::OpenOptionsExt, path::Path,
};

pub(super) const LIBRARIES: [&str; 3] = [
    "libbrotlienc.so.1",
    "libbrotlidec.so.1",
    "libbrotlicommon.so.1",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Manifest {
    pub abi_version: u32,
    pub target: String,
    pub brotli_version: u32,
    pub files: BTreeMap<String, String>,
}

pub(super) struct Bundle {
    pub manifest: Manifest,
    pub manifest_bytes: Vec<u8>,
    pub libraries: [Vec<u8>; 3],
}

pub(super) fn target() -> Result<String, Error> {
    if cfg!(all(target_os = "linux", target_env = "gnu")) {
        Ok(format!("{}-unknown-linux-gnu", std::env::consts::ARCH))
    } else {
        Err(Error::Invalid("unsupported Brotli target".into()))
    }
}

fn read(path: &Path, maximum: u64) -> Result<Vec<u8>, Error> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(Error::Invalid(format!(
            "not a regular file: {}",
            path.display()
        )));
    }
    let mut bytes = Vec::new();
    file.by_ref().take(maximum + 1).read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).map_err(|_| Error::Invalid("file size overflow".into()))?
        > maximum
    {
        return Err(Error::Invalid(format!(
            "file too large: {}",
            path.display()
        )));
    }
    Ok(bytes)
}

fn elf(bytes: &[u8]) -> Result<(), Error> {
    let machine = match std::env::consts::ARCH {
        "x86_64" => 62_u16,
        "aarch64" => 183_u16,
        arch => {
            return Err(Error::Invalid(format!(
                "unsupported Brotli architecture: {arch}"
            )))
        }
    };
    if bytes.get(..6) != Some(b"\x7fELF\x02\x01")
        || bytes.get(16..18) != Some(3_u16.to_le_bytes().as_slice())
        || bytes.get(18..20) != Some(machine.to_le_bytes().as_slice())
    {
        return Err(Error::Invalid("Brotli library ELF target mismatch".into()));
    }
    Ok(())
}

impl Bundle {
    pub fn read(root: &Path) -> Result<Self, Error> {
        let manifest_bytes = read(&root.join("manifest.json"), 65_536)?;
        let manifest: Manifest = serde_json::from_slice(&manifest_bytes)?;
        if manifest.abi_version != 1
            || manifest.target != target()?
            || manifest.brotli_version >> 24 != 1
        {
            return Err(Error::Invalid(
                "Brotli manifest target/version mismatch".into(),
            ));
        }
        if manifest.files.len() != LIBRARIES.len()
            || LIBRARIES
                .iter()
                .any(|name| !manifest.files.contains_key(*name))
        {
            return Err(Error::Invalid(
                "Brotli manifest must name exactly three libraries".into(),
            ));
        }
        let mut libraries = [Vec::new(), Vec::new(), Vec::new()];
        for (name, bytes) in LIBRARIES.into_iter().zip(&mut libraries) {
            *bytes = read(&root.join(name), 16 * 1024 * 1024)?;
            elf(bytes)?;
            let digest = format!("{:x}", Sha256::digest(&*bytes));
            if manifest.files.get(name) != Some(&digest) {
                return Err(Error::Invalid(format!("Brotli checksum mismatch: {name}")));
            }
        }
        Ok(Self {
            manifest,
            manifest_bytes,
            libraries,
        })
    }
}
