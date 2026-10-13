//! Immutable native provider data for the exact source-pinned generic model.
mod copy;
mod digest_cache;
mod record;

use super::{assets, pinned_manifest, Error};
use crate::model::{Manifest, Paths};
use record::Record;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn supported(model: &Manifest) -> bool {
    let pinned = pinned_manifest();
    model.sha256 == pinned.sha256 && model.size == pinned.size && model.precompiled_only()
}

pub fn seed(paths: &Paths, model: &Manifest) -> Result<bool, Error> {
    if !supported(model) {
        return Ok(false);
    }
    let Some(record) = Record::read(&paths.assets)? else {
        return Ok(false);
    };
    record.package(&paths.assets)?;
    let source = record.model(&paths.assets)?;
    copy::materialize(&source, &paths.cache.join(model.cache_filename()), model)?;
    Ok(true)
}

pub fn installed(model: &Manifest, cache: &Path, assets: &Path) -> Result<Option<PathBuf>, Error> {
    if !supported(model) {
        return Ok(None);
    }
    let root = cache.join("precompiled").join(&model.sha256);
    if root.join("rejected").exists() {
        return Ok(None);
    }
    let Some(record) = Record::read(&root)? else {
        return Ok(None);
    };
    if Record::read(assets)?.as_ref() != Some(&record) {
        return Err(Error::Invalid(
            "cached native provider differs from installed package".into(),
        ));
    }
    record.package(assets)?;
    record.model(assets)?;
    let path = root.join("model.pkl");
    Ok(digest_cache::verified(&path)?.then_some(path))
}

pub fn ensure(model: &Manifest, cache: &Path, assets: &Path) -> Result<Option<PathBuf>, Error> {
    if !supported(model) {
        return Ok(None);
    }
    let Some(record) = Record::read(assets)? else {
        return Ok(None);
    };
    record.package(assets)?;
    let source = record.model(assets)?;
    let root = cache.join("precompiled").join(&model.sha256);
    if fs::read_to_string(root.join("rejected")).is_ok_and(|value| value == model.sha256) {
        return Ok(None);
    }
    let target = root.join("model.pkl");
    copy::materialize(&source, &target, model)?;
    record.write(&root)?;
    Ok(Some(target))
}

pub(crate) fn present(assets: &Path, model: &Manifest) -> Result<bool, Error> {
    Ok(supported(model) && Record::read(assets)?.is_some())
}

pub(crate) fn worker_package(model: &Path, assets: &Path) -> Result<assets::Package, Error> {
    let root = model
        .parent()
        .ok_or_else(|| Error::Invalid("installed native model has no directory".into()))?;
    let record = Record::read(root)?
        .ok_or_else(|| Error::Invalid("installed native provider metadata missing".into()))?;
    if model.file_name() != Some(std::ffi::OsStr::new("model.pkl"))
        || Record::read(assets)?.as_ref() != Some(&record)
    {
        return Err(Error::Invalid(
            "worker native provider binding mismatch".into(),
        ));
    }
    record.model(assets)?;
    record.package(assets)
}

pub(crate) fn identity(model: &Path) -> Result<serde_json::Value, Error> {
    let root = model
        .parent()
        .ok_or_else(|| Error::Invalid("installed model has no directory".into()))?;
    match fs::read(root.join("installed.json")) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let record = Record::read(root)?.ok_or_else(|| {
                Error::Invalid("installed native provider metadata missing".into())
            })?;
            Ok(serde_json::to_value(record)?)
        }
        Err(error) => Err(error.into()),
    }
}
