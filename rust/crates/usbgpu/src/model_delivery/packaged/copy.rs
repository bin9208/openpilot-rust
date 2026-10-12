use super::super::{sha256, Error};
use crate::model::Manifest;
use std::{
    fs::File,
    io::{self, Write},
    path::Path,
};

pub(super) fn verified(path: &Path, model: &Manifest) -> Result<bool, Error> {
    let metadata = match path.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    Ok(metadata.is_file() && metadata.len() == model.size && sha256(path)? == model.sha256)
}

pub(super) fn materialize(source: &Path, target: &Path, model: &Manifest) -> Result<(), Error> {
    if verified(target, model)? {
        return Ok(());
    }
    let parent = target
        .parent()
        .ok_or_else(|| Error::Invalid("native model cache has no parent".into()))?;
    std::fs::create_dir_all(parent)?;
    let capacity = rustix::fs::statvfs(parent).map_err(io::Error::from)?;
    if capacity.f_bavail.saturating_mul(capacity.f_frsize) < model.size.saturating_add(256 << 20) {
        return Err(Error::Invalid(
            "insufficient storage for packaged model cache".into(),
        ));
    }
    let mut stage = tempfile::Builder::new()
        .prefix(".native-model-")
        .tempfile_in(parent)?;
    let copied = io::copy(&mut File::open(source)?, &mut stage)?;
    stage.flush()?;
    stage.as_file().sync_all()?;
    if copied != model.size || sha256(stage.path())? != model.sha256 {
        return Err(Error::Invalid(
            "packaged native PKL checksum mismatch".into(),
        ));
    }
    stage.persist(target).map_err(|error| error.error)?;
    Ok(())
}
