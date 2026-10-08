use super::Catalog;
use crate::Value;
use std::os::unix::fs::MetadataExt;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, OnceLock},
};

type Signature = (i128, u64);
type Entries = BTreeMap<PathBuf, (Option<Signature>, Catalog)>;
static CACHE: OnceLock<Mutex<Entries>> = OnceLock::new();

fn cache() -> MutexGuard<'static, Entries> {
    CACHE
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}
fn signature(path: &Path) -> std::io::Result<Signature> {
    let metadata = fs::metadata(path)?;
    Ok((
        i128::from(metadata.mtime()) * 1_000_000_000 + i128::from(metadata.mtime_nsec()),
        metadata.len(),
    ))
}
fn absolute(path: &Path) -> std::io::Result<PathBuf> {
    let path = if path.is_absolute() {
        path.into()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::CurDir => (),
            std::path::Component::RootDir
            | std::path::Component::Prefix(_)
            | std::path::Component::Normal(_) => normalized.push(component),
        }
    }
    Ok(normalized)
}

pub fn clear_catalog_cache(path: Option<&Path>) -> std::io::Result<()> {
    match path {
        Some(path) => {
            cache().remove(&absolute(path)?);
        }
        None => cache().clear(),
    }
    Ok(())
}

pub fn load_catalog(path: &Path) -> Catalog {
    let path = match absolute(path) {
        Ok(path) => path,
        Err(error) => {
            eprintln!(
                "Using safe drive content catalog for {}: {error}",
                path.display()
            );
            return Catalog::safe();
        }
    };
    for attempt in 0..2 {
        let before = signature(&path);
        let before_signature = before.as_ref().ok().copied();
        if let Some((_, catalog)) = cache()
            .get(&path)
            .filter(|(signature, _)| *signature == before_signature)
        {
            return catalog.clone();
        }
        let result = match before {
            Err(error) => Err(error.to_string()),
            Ok(_) => {
                let read = fs::read_to_string(&path)
                    .map_err(|error| error.to_string())
                    .and_then(|text| Value::parse(&text).map_err(|error| error.to_string()));
                let after = signature(&path);
                let after_signature = after.as_ref().ok().copied();
                if after_signature != before_signature {
                    if attempt == 0 {
                        continue;
                    }
                    eprintln!("Using safe drive content catalog for {}: catalog changed while reading: before={before_signature:?} after={after_signature:?}", path.display());
                    return Catalog::safe();
                }
                match after {
                    Err(error) => Err(error.to_string()),
                    Ok(_) => read.and_then(|value| {
                        Catalog::validate(&value).map_err(|error| error.to_string())
                    }),
                }
            }
        };
        let catalog = match result {
            Ok(catalog) => catalog,
            Err(error) => {
                eprintln!(
                    "Using safe drive content catalog for {}: {error}",
                    path.display()
                );
                Catalog::safe()
            }
        };
        cache().insert(path.clone(), (before_signature, catalog.clone()));
        return catalog;
    }
    Catalog::safe()
}
