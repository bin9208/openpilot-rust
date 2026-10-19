//! Persist exact App-selected model contracts after protocol and engine validation.
use crate::{model_spec::ModelSpec, Error};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

pub fn approved(cache: &Path) -> Result<Option<ModelSpec>, Error> {
    let file = match fs::File::open(cache.join("selected.json")) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut bytes = Vec::new();
    file.take(65537).read_to_end(&mut bytes)?;
    Ok(Some(ModelSpec::parse(&bytes)?))
}
pub fn remember(cache: &Path, spec: &ModelSpec) -> Result<(), Error> {
    spec.validate()?;
    if let Some(old) = approved(cache)? {
        if old.sha256 == spec.sha256 {
            if old == *spec {
                return Ok(());
            }
            return Err(Error::Contract("model contract changed under approved SHA"));
        }
    }
    fs::create_dir_all(cache)?;
    let mut staging = tempfile::NamedTempFile::new_in(cache)?;
    serde_json::to_writer(staging.as_file_mut(), spec)?;
    staging.flush()?;
    staging.as_file().sync_all()?;
    staging
        .persist(cache.join("selected.json"))
        .map_err(|error| error.error)?;
    Ok(())
}
pub fn check_loaded(spec: &ModelSpec, telemetry: &serde_json::Value) -> Result<(), Error> {
    if telemetry.as_object().is_some_and(|value| {
        value
            .get("loaded")
            .is_some_and(|loaded| loaded != &spec.sha256)
    }) {
        return Err(Error::Contract("App model changed or unloaded"));
    }
    Ok(())
}
