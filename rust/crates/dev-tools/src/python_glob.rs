//! Small Unix Python 3.12 Path.glob selector for literal/single-star components.
use crate::seal::Result;
use std::{
    fs,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};
/// Results retain the supplied base's lexical relative paths. Every component,
/// including literals, scans its parent; final entries are not stat'ed here.
pub fn glob(cwd: &Path, base: &Path, pattern: &str) -> Result<Vec<PathBuf>> {
    if base.as_os_str().as_bytes().contains(&0) {
        return Ok(vec![]);
    }
    let parts = pattern.split('/').map(str::as_bytes).collect::<Vec<_>>();
    if parts.is_empty()
        || parts.iter().any(|part| {
            part.is_empty()
                || *part == b"."
                || *part == b".."
                || part.contains(&b'?')
                || part.contains(&b'[')
                || part.iter().filter(|&&b| b == b'*').count() > 1
        })
    {
        return Err("unsupported Python glob pattern".into());
    }
    let physical = |path: &Path| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            cwd.join(path)
        }
    };
    match fs::metadata(physical(base)) {
        Ok(meta) if !meta.is_dir() => return Ok(vec![]),
        Ok(_) => {}
        Err(error)
            if error.raw_os_error().is_some_and(|code| {
                [
                    rustix::io::Errno::NOENT,
                    rustix::io::Errno::NOTDIR,
                    rustix::io::Errno::BADF,
                    rustix::io::Errno::LOOP,
                ]
                .iter()
                .any(|errno| errno.raw_os_error() == code)
            }) =>
        {
            return Ok(vec![]);
        }
        Err(error) => return Err(error.into()),
    }
    fn matches(name: &[u8], pattern: &[u8]) -> bool {
        if let Some(at) = pattern.iter().position(|&b| b == b'*') {
            name.starts_with(&pattern[..at])
                && name.ends_with(&pattern[at + 1..])
                && name.len() >= pattern.len() - 1
        } else {
            name == pattern
        }
    }
    fn select(cwd: &Path, parent: &Path, parts: &[&[u8]], found: &mut Vec<PathBuf>) {
        let physical = if parent.is_absolute() {
            parent.to_path_buf()
        } else {
            cwd.join(parent)
        };
        // list(scandir) is inside Python's OSError handler, and the directory
        // handle is closed before DFS. An iterator error discards this branch.
        let Ok(entries) =
            fs::read_dir(physical).and_then(|v| v.collect::<std::io::Result<Vec<_>>>())
        else {
            return;
        };
        for entry in entries {
            let path = parent.join(entry.file_name());
            if parts.len() > 1 {
                let Ok(meta) = fs::metadata(entry.path()) else {
                    continue;
                };
                if !meta.is_dir() {
                    continue;
                }
            }
            if matches(entry.file_name().as_bytes(), parts[0]) {
                if parts.len() == 1 {
                    found.push(path);
                } else {
                    select(cwd, &path, &parts[1..], found);
                }
            }
        }
    }
    let mut found = vec![];
    select(cwd, base, &parts, &mut found);
    Ok(found)
}
