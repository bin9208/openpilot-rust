use super::{super::Error, copy};
use crate::model::Manifest;
use parking_lot::Mutex;
use std::{
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::OnceLock,
};

#[derive(Clone, PartialEq, Eq)]
struct Stamp {
    path: PathBuf,
    digest: String,
    device: u64,
    inode: u64,
    bytes: u64,
    modified: [i64; 2],
    changed: [i64; 2],
}

fn stamp(path: &Path, model: &Manifest) -> Result<Option<Stamp>, Error> {
    let metadata = match path.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() || metadata.len() != model.size {
        return Ok(None);
    }
    Ok(Some(Stamp {
        path: path.to_path_buf(),
        digest: model.sha256.clone(),
        device: metadata.dev(),
        inode: metadata.ino(),
        bytes: metadata.len(),
        modified: [metadata.mtime(), metadata.mtime_nsec()],
        changed: [metadata.ctime(), metadata.ctime_nsec()],
    }))
}

static VERIFIED: OnceLock<Mutex<[Option<Stamp>; 2]>> = OnceLock::new();

pub(super) fn verified(path: &Path, model: &Manifest) -> Result<bool, Error> {
    let Some(before) = stamp(path, model)? else {
        return Ok(false);
    };
    let cache = VERIFIED.get_or_init(|| Mutex::new([None, None]));
    if cache.lock().iter().flatten().any(|entry| entry == &before) {
        return Ok(true);
    }
    if !copy::verified(path, model)? {
        return Ok(false);
    }
    if stamp(path, model)?.as_ref() != Some(&before) {
        return Err(Error::Invalid(
            "native model changed during readiness admission".into(),
        ));
    }
    let mut entries = cache.lock();
    entries.swap(0, 1);
    entries[0] = Some(before);
    Ok(true)
}
