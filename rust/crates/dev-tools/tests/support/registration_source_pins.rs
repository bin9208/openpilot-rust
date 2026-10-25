//! Only the four retired helper names may be absent from copied test roots.
//! No archived Python implementation is compiled or reconstructed by these tests.
use std::{fs, io, path::Path};
pub fn retired(name: &str) -> bool {
    matches!(
        name,
        "rust/tools/check_registration.py"
            | "rust/tools/registration_source.py"
            | "rust/tools/check_navd_config.py"
            | "rust/tools/navd_config_source.py"
    )
}
pub fn read(root: &Path, name: &str) -> Option<Vec<u8>> {
    let path = root.join(name);
    match fs::symlink_metadata(&path) {
        Err(error) if retired(name) && error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => panic!("required test source {name}: {error}"),
        Ok(_) => Some(fs::read(path).unwrap()),
    }
}
