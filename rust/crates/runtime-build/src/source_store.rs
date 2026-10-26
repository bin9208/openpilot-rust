//! Retained, independently owned source checkouts. No runtime is activated here.
//!
//! The source worktrees belong to bare repositories in this store, never to the
//! mutable updater checkout. A failed new checkout cannot replace a previous one.
use crate::{regular_file, source, Result, Source};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Component, Path, PathBuf},
    process::Command,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub schema: u32,
    pub source: Source,
    pub root: PathBuf,
    pub modules: BTreeMap<PathBuf, Source>,
}

fn directory(path: &Path) -> Result<()> {
    match fs::create_dir(path) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    if !fs::symlink_metadata(path)?.is_dir() {
        return Err("source store requires real directories".into());
    }
    Ok(())
}

fn git(args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Result<String> {
    let output = Command::new("/usr/bin/git")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("LC_ALL", "C")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(["-c", "core.hooksPath=/dev/null", "-c", "core.fsync=all"])
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(format!("retained source Git operation failed: {}", output.status).into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|c| matches!(c, Component::Normal(name) if name != ".git"))
}

fn modules(root: &Path) -> Result<BTreeMap<PathBuf, Source>> {
    let mut result = BTreeMap::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(parent) = pending.pop() {
        let checkout = root.join(&parent);
        let listing = git([
            OsStr::new("-C"),
            checkout.as_os_str(),
            OsStr::new("ls-files"),
            OsStr::new("--stage"),
            OsStr::new("-z"),
        ])?;
        for entry in listing.split('\0').filter(|s| !s.is_empty()) {
            let (metadata, name) = entry.split_once('\t').ok_or("invalid Git index entry")?;
            let fields: Vec<_> = metadata.split(' ').collect();
            if fields.first() != Some(&"160000") {
                continue;
            }
            if fields.len() != 3 || fields[2] != "0" || !relative(Path::new(name)) {
                return Err("unmerged or unsafe source submodule".into());
            }
            let path = parent.join(name);
            let child = root.join(&path);
            // Missing, redirected and uninitialized modules cannot look complete.
            if child.canonicalize()? != child {
                return Err("source submodule path is redirected".into());
            }
            let identity = source::inspect(&child)?;
            if identity.commit != fields[1]
                || result.len() >= 256
                || result.insert(path.clone(), identity).is_some()
            {
                return Err("source submodule commit mismatch or excessive module graph".into());
            }
            pending.push(path);
        }
    }
    Ok(result)
}

fn repository(store: &Path, module: &Path) -> PathBuf {
    // Paths are UTF-8 because they came from the bounded Git index decoder.
    let key = format!(
        "{:x}",
        Sha256::digest(module.as_os_str().as_encoded_bytes())
    );
    store.join("repositories").join(format!("{key}.git"))
}

fn checkout(
    store: &Path,
    input: &Path,
    destination: &Path,
    module: &Path,
    sha: &str,
) -> Result<()> {
    let repository = repository(store, module);
    if !repository.exists() {
        git([
            OsStr::new("init"),
            OsStr::new("--bare"),
            OsStr::new("--template="),
            OsStr::new("--initial-branch=unused"),
            repository.as_os_str(),
        ])?;
    }
    directory(&repository)?;
    let reference = format!("{sha}:refs/openpilot/{sha}");
    git([
        OsStr::new("--git-dir"),
        repository.as_os_str(),
        OsStr::new("fetch"),
        OsStr::new("--no-tags"),
        OsStr::new("--no-write-fetch-head"),
        OsStr::new("--"),
        input.as_os_str(),
        OsStr::new(&reference),
    ])?;
    git([
        OsStr::new("--git-dir"),
        repository.as_os_str(),
        OsStr::new("worktree"),
        OsStr::new("add"),
        OsStr::new("--detach"),
        OsStr::new("--lock"),
        OsStr::new("--reason=retained-runtime-source"),
        destination.as_os_str(),
        OsStr::new(sha),
    ])?;
    Ok(())
}

impl Snapshot {
    /// Revalidate before use, including the independent Git ownership and modules.
    pub fn verify(&self, store: &Path) -> Result<()> {
        let store = store.canonicalize()?;
        if self.schema != 1
            || self.root.parent() != Some(store.join("checkouts").as_path())
            || self.root.canonicalize()? != self.root
            || source::inspect(&self.root)? != self.source
            || modules(&self.root)? != self.modules
        {
            return Err("retained source receipt does not match its checkout".into());
        }
        for module in
            std::iter::once(Path::new("")).chain(self.modules.keys().map(PathBuf::as_path))
        {
            let root = self.root.join(module);
            let common = git([
                OsStr::new("-C"),
                root.as_os_str(),
                OsStr::new("rev-parse"),
                OsStr::new("--path-format=absolute"),
                OsStr::new("--git-common-dir"),
            ])?;
            if Path::new(common.trim_end()).canonicalize()? != repository(&store, module) {
                return Err("retained source still depends on a foreign Git directory".into());
            }
        }
        Ok(())
    }
}

/// Import local committed objects and materialize a stable source root. Network
/// acquisition, compilation, runtime sealing and active selection are separate.
/// The caller supplies this operation's estimated extra disk requirement.
pub fn retain(input: &Path, store: &Path, expected_growth_bytes: u64) -> Result<Snapshot> {
    let input = input.canonicalize()?;
    let source = source::inspect(&input)?;
    let expected_modules = modules(&input)?;
    directory(store)?;
    let store = store.canonicalize()?;
    if store.starts_with(&input) || input.starts_with(&store) {
        return Err("retained source store and mutable checkout must be independent".into());
    }
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(store.join("source.lock"))?;
    if !lock.metadata()?.is_file() {
        return Err("source store lock must be a regular file".into());
    }
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive)?;
    for name in ["repositories", "checkouts", "receipts"] {
        directory(&store.join(name))?;
    }
    let receipt = store
        .join("receipts")
        .join(format!("{}.json", source.commit));
    if fs::symlink_metadata(&receipt).is_ok() {
        let file = regular_file(&receipt)?;
        if file.metadata()?.len() > 1024 * 1024 {
            return Err("retained source receipt exceeds 1 MiB".into());
        }
        let retained: Snapshot = serde_json::from_reader(file.take(1024 * 1024 + 1))?;
        if retained.source != source || retained.modules != expected_modules {
            return Err("retained source identity conflict".into());
        }
        retained.verify(&store)?;
        return Ok(retained);
    }
    let space = rustix::fs::statvfs(&store)?;
    let available = space
        .f_bavail
        .checked_mul(space.f_frsize)
        .ok_or("source space overflow")?;
    if expected_growth_bytes == 0 || available < expected_growth_bytes {
        return Err("insufficient space for this source import; previous sources preserved".into());
    }
    // Keep this exact path: renaming a Git worktree would invalidate its gitdir.
    // Interrupted, unreceipted attempts are never selected or removed here.
    let root = tempfile::Builder::new()
        .prefix("source-")
        .tempdir_in(store.join("checkouts"))?
        .keep();
    checkout(&store, &input, &root, Path::new(""), &source.commit)?;
    for (module, identity) in &expected_modules {
        checkout(
            &store,
            &input.join(module),
            &root.join(module),
            module,
            &identity.commit,
        )?;
    }
    let snapshot = Snapshot {
        schema: 1,
        source,
        root,
        modules: expected_modules,
    };
    snapshot.verify(&store)?;
    if source::inspect(&input)? != snapshot.source || modules(&input)? != snapshot.modules {
        return Err("source changed during retention; no receipt published".into());
    }
    let parent = store.join("receipts");
    let mut temporary = tempfile::NamedTempFile::new_in(&parent)?;
    serde_json::to_writer_pretty(&mut temporary, &snapshot)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(receipt)?;
    File::open(parent)?.sync_all()?;
    Ok(snapshot)
}
