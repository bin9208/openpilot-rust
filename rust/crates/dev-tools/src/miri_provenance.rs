//! Hash the current Miri sysroot's core libraries before publishing provenance.
use crate::{
    python_fs,
    seal::{self, Result},
};
use std::{
    ffi::{OsStr, OsString},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};

// pathlib normalizes repeated separators and dot components, while preserving
// parent components and the POSIX exactly-two-leading-slashes spelling.
fn lexical(path: &OsStr) -> PathBuf {
    let bytes = path.as_bytes();
    let mut out = if bytes.starts_with(b"//") && !bytes.starts_with(b"///") {
        b"//".to_vec()
    } else if bytes.starts_with(b"/") {
        b"/".to_vec()
    } else {
        Vec::new()
    };
    for part in bytes
        .split(|byte| *byte == b'/')
        .filter(|part| !part.is_empty() && *part != b".")
    {
        if !out.is_empty() && !out.ends_with(b"/") {
            out.push(b'/');
        }
        out.extend_from_slice(part);
    }
    OsString::from_vec(out).into()
}

/// Match pathlib's non-recursive glob, including hidden entries and symlinked
/// architecture directories. Traversal errors produce no matches; selected
/// unreadable files fail the whole operation without partial JSON publication.
pub fn core_hashes(root: &OsStr) -> Result<String> {
    let root = lexical(root);
    let mut paths =
        crate::python_glob::glob(Path::new("."), &root, "lib/rustlib/*/lib/libcore-*.rlib")?;
    // PosixPath ordering compares decoded path components, not raw bytes or
    // complete strings (an architecture's prefix sorts before its suffixes).
    fn key(path: &Path) -> Vec<Vec<u32>> {
        path.components()
            .map(|part| python_fs::codepoints(part.as_os_str()))
            .collect()
    }
    paths.sort_by_cached_key(|path| key(path));
    if paths.is_empty() {
        return Err("Miri sysroot contains no core libraries".into());
    }
    let mut entries = Vec::new();
    for path in paths {
        entries.push(format!(
            "  {}: \"{}\"",
            python_fs::json_string(path.as_os_str())?,
            seal::digest(&path)?
        ));
    }
    Ok(format!("{{\n{}\n}}\n", entries.join(",\n")))
}
