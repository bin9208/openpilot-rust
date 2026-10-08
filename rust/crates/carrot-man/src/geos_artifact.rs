use crate::Error;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    schema_version: u32,
    geos_version: String,
    architecture: String,
    libraries: Vec<Artifact>,
}
#[derive(Deserialize)]
struct Artifact {
    name: String,
    sha256: String,
}

pub(super) fn verify(path: &Path, bytes: &[u8]) -> Result<PathBuf, Error> {
    let manifest: Manifest = serde_json::from_slice(bytes)?;
    if manifest.schema_version != 1
        || manifest.geos_version != "3.13.1-CAPI-1.19.2"
        || manifest.architecture != std::env::consts::ARCH
        || manifest.libraries.len() != 2
    {
        return Err(Error::Geos(
            "artifact manifest ABI or architecture mismatch".into(),
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| Error::Geos("artifact directory is missing".into()))?;
    let machine = match std::env::consts::ARCH {
        "x86_64" => 62_u16,
        "aarch64" => 183_u16,
        _ => return Err(Error::Geos("unsupported ELF machine".into())),
    };
    let mut dependency = None;
    let mut matched_c = false;
    for artifact in manifest.libraries {
        if Path::new(&artifact.name).components().count() != 1 {
            return Err(Error::Geos("invalid artifact filename".into()));
        }
        let artifact_path = parent.join(&artifact.name);
        let bytes = std::fs::read(&artifact_path)?;
        if format!("{:x}", Sha256::digest(&bytes)) != artifact.sha256 {
            return Err(Error::Geos("artifact SHA256 mismatch".into()));
        }
        if bytes.len() < 20
            || &bytes[..4] != b"\x7fELF"
            || bytes[4] != 2
            || bytes[5] != 1
            || u16::from_le_bytes([bytes[18], bytes[19]]) != machine
        {
            return Err(Error::Geos("artifact ELF architecture mismatch".into()));
        }
        if artifact.name.starts_with("libgeos_c-") && artifact.name.ends_with(".so.1.19.2") {
            if path.file_name() != artifact_path.file_name() {
                return Err(Error::Geos("C API artifact filename mismatch".into()));
            }
            matched_c = true;
        } else if artifact.name.starts_with("libgeos-") && artifact.name.ends_with(".so.3.13.1") {
            dependency = Some(artifact_path);
        } else {
            return Err(Error::Geos("unexpected artifact name".into()));
        }
    }
    if !matched_c {
        return Err(Error::Geos("C API artifact is missing".into()));
    }
    dependency.ok_or_else(|| Error::Geos("GEOS implementation artifact is missing".into()))
}
