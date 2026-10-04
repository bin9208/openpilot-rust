use crate::Error;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, path::Path};

const FILES: [&str; 7] = [
    "libacados.so",
    "libblasfeo.so",
    "libhpipm.so",
    "libqpOASES_e.so.3.1",
    "libqpOASES_e.so",
    "libacados_ocp_solver_lat.so",
    "libacados_ocp_solver_long.so",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: u32,
    acados: String,
    abi: String,
    architecture: String,
    wheel_sha256: String,
    files: Vec<Artifact>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    name: String,
    sha256: String,
}

pub(super) fn verify(directory: &Path) -> Result<(), Error> {
    let manifest: Manifest = serde_json::from_slice(&fs::read(directory.join("manifest.json"))?)?;
    let wheel = match std::env::consts::ARCH {
        "x86_64" => "3b451852e83d62815cead999ab31073db9be60307f772650336cdd4534f12b9e",
        "aarch64" => "2ad9fcebef1f65112a9cebe8a093977310602e1325d442dc345beaf512679211",
        _ => return Err(Error::Contract("unsupported acados architecture")),
    };
    if manifest.format != 1
        || manifest.acados != "0.2.2.post103"
        || manifest.abi != "ocp-double-i32"
        || manifest.architecture != std::env::consts::ARCH
        || manifest.wheel_sha256 != wheel
        || manifest.files.len() != FILES.len()
        || manifest
            .files
            .iter()
            .map(|file| file.name.as_str())
            .collect::<BTreeSet<_>>()
            != FILES.into_iter().collect()
    {
        return Err(Error::Contract("pinned acados artifact manifest required"));
    }
    for file in &manifest.files {
        if format!(
            "{:x}",
            Sha256::digest(fs::read(directory.join(&file.name))?)
        ) != file.sha256
        {
            return Err(Error::Contract("acados artifact hash mismatch"));
        }
    }
    Ok(())
}
