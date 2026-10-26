//! Receipts for unpacked toolchains, provider headers/libraries and generated inputs.
use crate::{digest_file, regular_file, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Entry {
    Directory,
    File {
        sha256: String,
        bytes: u64,
        mode: u32,
    },
    Symlink {
        target: PathBuf,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Tree {
    pub schema: u32,
    pub entries: BTreeMap<PathBuf, Entry>,
}

impl Tree {
    pub fn read(path: &Path) -> Result<Self> {
        const LIMIT: u64 = 16 * 1024 * 1024;
        let file = regular_file(path)?;
        if file.metadata()?.len() > LIMIT {
            return Err("build input tree receipt exceeds 16 MiB".into());
        }
        let tree: Self = serde_json::from_reader(file.take(LIMIT + 1))?;
        if tree.schema != 1 || tree.entries.len() > 100_000 {
            return Err("invalid build input tree receipt".into());
        }
        Ok(tree)
    }

    pub fn capture(root: &Path) -> Result<Self> {
        if !fs::symlink_metadata(root)?.is_dir() {
            return Err("build input tree must be a real directory".into());
        }
        let root = root.canonicalize()?;
        let mut pending = vec![PathBuf::new()];
        let mut entries = BTreeMap::new();
        while let Some(directory) = pending.pop() {
            for child in fs::read_dir(root.join(&directory))? {
                let child = child?;
                let relative = directory.join(child.file_name());
                let path = root.join(&relative);
                let metadata = fs::symlink_metadata(&path)?;
                let entry = if metadata.is_dir() {
                    pending.push(relative.clone());
                    Entry::Directory
                } else if metadata.is_file() {
                    let mode = metadata.permissions().mode() & 0o7777;
                    if mode & 0o6000 != 0 {
                        return Err("set-id build input is unsupported".into());
                    }
                    Entry::File {
                        sha256: digest_file(&path)?,
                        bytes: metadata.len(),
                        mode,
                    }
                } else if metadata.is_symlink() {
                    let target = fs::read_link(&path)?;
                    if target.is_absolute()
                        || !path.canonicalize()?.starts_with(&root)
                        || !path.is_file()
                    {
                        return Err(
                            "build input symlink must resolve to a file inside its tree".into()
                        );
                    }
                    Entry::Symlink { target }
                } else {
                    return Err("unsupported build input tree member".into());
                };
                entries.insert(relative, entry);
                if entries.len() > 100_000 {
                    return Err("build input tree exceeds 100000 members".into());
                }
            }
        }
        if entries.is_empty() {
            return Err("empty build input tree".into());
        }
        Ok(Self { schema: 1, entries })
    }

    /// Compare the complete actual tree, including additions, links and modes.
    pub fn verify(&self, root: &Path) -> Result<()> {
        if *self != Self::capture(root)? {
            return Err("unpacked build input tree differs from its verified receipt".into());
        }
        Ok(())
    }
}

/// Only compiler inputs accept the explicit live-system SDK receipt. Ordinary
/// provider/model/toolchain Trees retain their stricter original link policy.
pub enum Receipt {
    Tree(Tree),
    SystemSdk(crate::system_sdk::Snapshot),
}

impl Receipt {
    pub fn read(path: &Path) -> Result<Self> {
        const LIMIT: u64 = 16 * 1024 * 1024;
        let file = regular_file(path)?;
        if file.metadata()?.len() > LIMIT {
            return Err("build input receipt exceeds 16 MiB".into());
        }
        let value: serde_json::Value = serde_json::from_reader(file.take(LIMIT + 1))?;
        if value.get("kind").and_then(serde_json::Value::as_str) == Some(crate::system_sdk::KIND) {
            let snapshot: crate::system_sdk::Snapshot = serde_json::from_value(value)?;
            snapshot.validate()?;
            Ok(Self::SystemSdk(snapshot))
        } else {
            let tree: Tree = serde_json::from_value(value)?;
            if tree.schema != 1 || tree.entries.len() > 100_000 {
                return Err("invalid build input tree receipt".into());
            }
            Ok(Self::Tree(tree))
        }
    }

    pub fn verify(&self, root: &Path) -> Result<()> {
        match self {
            Self::Tree(tree) => tree.verify(root),
            Self::SystemSdk(snapshot) => {
                if root != Path::new("/") {
                    return Err("native compiler SDK must verify the live system root, not a different sysroot".into());
                }
                snapshot.verify(root)
            }
        }
    }

    pub(crate) fn files(&self, root: &Path) -> Result<BTreeMap<PathBuf, String>> {
        match self {
            Self::Tree(tree) => tree
                .entries
                .iter()
                .filter_map(|(p, e)| match e {
                    Entry::File { sha256, .. } => Some((p, sha256)),
                    _ => None,
                })
                .map(|(p, sha)| Ok((root.join(p).canonicalize()?, sha.clone())))
                .collect(),
            Self::SystemSdk(snapshot) => snapshot
                .entries
                .iter()
                .filter_map(|(p, e)| match e {
                    crate::system_sdk::Entry::File { sha256, .. } => Some((p, sha256)),
                    _ => None,
                })
                .map(|(p, sha)| Ok((root.join(p).canonicalize()?, sha.clone())))
                .collect(),
        }
    }
}
