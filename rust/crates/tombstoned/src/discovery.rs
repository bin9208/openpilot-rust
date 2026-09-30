use crate::Error;
use std::{
    collections::HashSet,
    fs,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct Tombstone {
    pub path: PathBuf,
    pub ctime: i64,
}
/// Best-effort remove nonhidden children, preserving glob('*') and swallowed removal errors.
pub fn clear_apport_folder(path: &Path) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.file_name().as_bytes().starts_with(b".") {
            // The source deliberately ignores every removal failure (directories, permissions, races).
            if let Err(_ignored_by_source) = fs::remove_file(entry.path()) {}
        }
    }
}
/// Scan exactly the first 1000 directory entries in filesystem enumeration order.
///
/// # Errors
/// Directory/stat failures propagate, including races after an entry was enumerated.
pub fn get_tombstones(path: &Path) -> Result<HashSet<Tombstone>, Error> {
    let mut files = HashSet::new();
    if !path.exists() {
        return Ok(files);
    }
    for entry in fs::read_dir(path)?.take(1000) {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.as_bytes();
        if name.starts_with(b"tombstone") || name.ends_with(b".crash") {
            let metadata = fs::metadata(entry.path())?;
            if name.starts_with(b"tombstone") || metadata.mode() == 0o100640 {
                // Python int(st_ctime) truncates the floating timestamp, including negative values.
                let ctime =
                    (metadata.ctime() as f64 + metadata.ctime_nsec() as f64 * 1e-9).trunc() as i64;
                files.insert(Tombstone {
                    path: entry.path(),
                    ctime,
                });
            }
        }
    }
    Ok(files)
}
