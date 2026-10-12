use super::{
    manifest::{Bundle, LIBRARIES},
    Error, Provider,
};
use std::{
    fs::{self, File},
    io::Write,
    os::unix::fs::symlink,
    path::Path,
};

fn write(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let mut file = File::create_new(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

pub(super) fn install(provider: &Provider) -> Result<(), Error> {
    let bundle = Bundle::read(&provider.bundle)?;
    fs::create_dir_all(&provider.active)?;
    let generation = tempfile::Builder::new()
        .prefix("generation-")
        .tempdir_in(&provider.active)?;
    for (name, bytes) in LIBRARIES.into_iter().zip(&bundle.libraries) {
        write(&generation.path().join(name), bytes)?;
    }
    write(
        &generation.path().join("manifest.json"),
        &bundle.manifest_bytes,
    )?;
    File::open(generation.path())?.sync_all()?;
    let verified = Bundle::read(generation.path())?;
    let codec = crate::static_web::brotli::Brotli::load_bundle(
        generation.path(),
        verified.manifest.brotli_version,
    )
    .ok_or(Error::Unavailable)?;
    let encoded = codec.compress(b"Carrot QR Brotli provider")?;
    if !codec.matches(&encoded, b"Carrot QR Brotli provider") {
        return Err(Error::Unavailable);
    }
    drop(codec);
    let activation = tempfile::Builder::new()
        .prefix(".activation-")
        .tempdir_in(&provider.active)?;
    let name = generation
        .path()
        .file_name()
        .ok_or_else(|| Error::Invalid("generation path missing name".into()))?;
    symlink(name, activation.path().join("current"))?;
    File::open(&provider.active)?.sync_all()?;
    fs::rename(
        activation.path().join("current"),
        provider.active.join("current"),
    )?;
    let retained = generation.keep();
    File::open(&provider.active)?
        .sync_all()
        .map_err(|error| Error::Activated {
            path: retained,
            source: error,
        })?;
    Ok(())
}
