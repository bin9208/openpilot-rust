//! Shared strict file-tree admission for authenticated native archives.
#[cfg(any(
    feature = "prepare-rust",
    feature = "prepare-capnp",
    feature = "prepare-cargo"
))]
use crate::inputs::Tree;
use crate::{inputs::Entry, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
#[cfg(any(
    feature = "prepare-rust",
    feature = "prepare-capnp",
    feature = "prepare-cargo"
))]
use std::collections::BTreeSet;
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Component as PathComponent, Path, PathBuf},
};

pub(crate) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) fn json<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub(crate) fn directory(path: &Path) -> Result<()> {
    match fs::create_dir(path) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    if !fs::symlink_metadata(path)?.is_dir() {
        return Err("native input store requires real directories".into());
    }
    Ok(())
}

pub(crate) fn normal(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|c| matches!(c, PathComponent::Normal(_)))
}

pub(crate) fn insert(
    entries: &mut BTreeMap<PathBuf, Entry>,
    path: PathBuf,
    entry: Entry,
) -> Result<()> {
    if !normal(&path) {
        return Err("unsafe native archive member path".into());
    }
    let mut parent = path.parent();
    while let Some(p) = parent.filter(|p| !p.as_os_str().is_empty()) {
        if entries.get(p).is_some_and(|e| e != &Entry::Directory) {
            return Err("native archive member overlaps a non-directory".into());
        }
        entries.insert(p.to_owned(), Entry::Directory);
        parent = p.parent();
    }
    if entries
        .get(&path)
        .is_some_and(|previous| previous != &entry)
    {
        return Err("conflicting native archive distribution members".into());
    }
    entries.insert(path, entry);
    if entries.len() > 100_000 {
        return Err("native archive member count exceeds the tree receipt limit".into());
    }
    Ok(())
}

pub(crate) fn parents(root: &Path, relative: &Path) -> Result<()> {
    let mut current = root.to_owned();
    for part in relative.parent().into_iter().flat_map(Path::components) {
        if !matches!(part, PathComponent::Normal(_)) {
            return Err("unsafe native archive parent".into());
        }
        current.push(part);
        directory(&current)?;
    }
    Ok(())
}

pub(crate) fn file(root: &Path, relative: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    parents(root, relative)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(root.join(relative))?;
    output.write_all(bytes)?;
    output.set_permissions(fs::Permissions::from_mode(mode))?;
    output.sync_all()?;
    Ok(())
}

#[cfg(any(
    feature = "prepare-rust",
    feature = "prepare-capnp",
    feature = "prepare-cargo"
))]
pub(crate) fn links(tree: &Tree, destination: Option<&Path>) -> Result<()> {
    for (path, value) in &tree.entries {
        let Entry::Symlink { target } = value else {
            continue;
        };
        let mut pending = path.parent().unwrap_or(Path::new("")).join(target);
        let mut visited = BTreeSet::new();
        loop {
            let mut resolved = PathBuf::new();
            for part in pending.components() {
                match part {
                    PathComponent::Normal(name) => resolved.push(name),
                    PathComponent::CurDir => (),
                    PathComponent::ParentDir if resolved.pop() => (),
                    _ => return Err("native archive symlink escapes the toolchain".into()),
                }
            }
            if !visited.insert(resolved.clone()) {
                return Err("native archive symlink cycle".into());
            }
            match tree.entries.get(&resolved) {
                Some(Entry::File { .. }) => break,
                Some(Entry::Symlink { target }) => {
                    pending = resolved.parent().unwrap_or(Path::new("")).join(target)
                }
                _ => return Err("native archive symlink must resolve to a contained file".into()),
            }
        }
        if let Some(root) = destination {
            parents(root, path)?;
            std::os::unix::fs::symlink(target, root.join(path))?;
        }
    }
    Ok(())
}

pub(crate) fn sync_directories(root: &Path) -> Result<()> {
    let mut pending = vec![root.to_owned()];
    while let Some(path) = pending.pop() {
        for item in fs::read_dir(&path)? {
            let item = item?;
            if item.file_type()?.is_dir() {
                pending.push(item.path());
            }
        }
        File::open(path)?.sync_all()?;
    }
    Ok(())
}
