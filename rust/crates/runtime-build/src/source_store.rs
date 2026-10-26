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

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct UpdateCheckout {
    schema: u32,
    source: Source,
    branch: String,
    root: PathBuf,
}

pub(crate) struct Workspace {
    pub root: PathBuf,
    _lock: File,
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
    add_worktree(store, destination, module, sha)
}

fn add_worktree(store: &Path, destination: &Path, module: &Path, sha: &str) -> Result<()> {
    let repository = repository(store, module);
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
    /// OverlayFS needs a complete .git directory inside its lower tree. A
    /// retained Git worktree has an external gitdir and must never be that tree.
    /// This small shared clone borrows only the store-owned immutable objects.
    pub fn update_checkout(
        &self,
        store: &Path,
        branch: &str,
        expected_growth_bytes: u64,
    ) -> Result<PathBuf> {
        self.verify(store)?;
        if !self.modules.is_empty() {
            return Err("native updater requires this repository's flattened source layout".into());
        }
        git(["check-ref-format", "--branch", branch])?;
        let store = store.canonicalize()?;
        let _lock = lock(&store)?;
        let parent = store.join("update-checkouts");
        directory(&parent)?;
        let key = format!(
            "{:x}",
            Sha256::digest(format!("{}:{branch}", self.source.commit).as_bytes())
        );
        let receipt = store.join("receipts").join(format!("update-{key}.json"));
        let checkout = if fs::symlink_metadata(&receipt).is_ok() {
            let input = regular_file(&receipt)?;
            if input.metadata()?.len() > 1024 * 1024 {
                return Err("update checkout receipt is excessive".into());
            }
            serde_json::from_reader::<_, UpdateCheckout>(input)?
        } else {
            admit(&store, expected_growth_bytes)?;
            let root = tempfile::Builder::new()
                .prefix("update-")
                .tempdir_in(&parent)?
                .keep();
            let repo = repository(&store, Path::new(""));
            git([
                OsStr::new("clone"),
                OsStr::new("--shared"),
                OsStr::new("--no-checkout"),
                OsStr::new("--template="),
                repo.as_os_str(),
                root.as_os_str(),
            ])?;
            git([
                OsStr::new("-C"),
                root.as_os_str(),
                OsStr::new("remote"),
                OsStr::new("set-url"),
                OsStr::new("origin"),
                OsStr::new("https://github.com/bin9208/openpilot-rust.git"),
            ])?;
            git([
                OsStr::new("-C"),
                root.as_os_str(),
                OsStr::new("checkout"),
                OsStr::new("-B"),
                OsStr::new(branch),
                OsStr::new(&self.source.commit),
            ])?;
            fs::create_dir_all(root.join(".git/info"))?;
            fs::write(
                root.join(".git/info/exclude"),
                b"/.overlay_init\n/.overlay_consistent\n",
            )?;
            let checkout = UpdateCheckout {
                schema: 1,
                source: self.source.clone(),
                branch: branch.into(),
                root,
            };
            verify_update_checkout(&checkout, self, &store)?;
            let mut temporary = tempfile::NamedTempFile::new_in(store.join("receipts"))?;
            serde_json::to_writer_pretty(&mut temporary, &checkout)?;
            temporary.write_all(b"\n")?;
            temporary.as_file().sync_all()?;
            temporary.persist_noclobber(&receipt)?;
            File::open(store.join("receipts"))?.sync_all()?;
            checkout
        };
        if checkout.branch != branch {
            return Err("native update checkout branch differs from its receipt".into());
        }
        verify_update_checkout(&checkout, self, &store)?;
        Ok(checkout.root)
    }

    /// Revalidate before use, including the independent Git ownership and modules.
    pub fn verify(&self, store: &Path) -> Result<()> {
        self.verify_with_metadata(store, None)
    }

    pub(crate) fn verify_with_metadata(
        &self,
        store: &Path,
        metadata_sha256: Option<&str>,
    ) -> Result<()> {
        self.verify_runtime(store, metadata_sha256, &BTreeMap::new())
    }

    pub(crate) fn verify_runtime(
        &self,
        store: &Path,
        metadata_sha256: Option<&str>,
        assets: &BTreeMap<PathBuf, String>,
    ) -> Result<()> {
        let store = store.canonicalize()?;
        if self.schema != 1
            || self.root.parent() != Some(store.join("checkouts").as_path())
            || self.root.canonicalize()? != self.root
            || source::inspect_runtime(&self.root, metadata_sha256, assets)? != self.source
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

    /// Each execution candidate gets its own root, including rebuilds of the
    /// same commit. Staging must never write into an already running checkout.
    pub(crate) fn fork(&self, store: &Path, expected_growth_bytes: u64) -> Result<Self> {
        self.verify(store)?;
        let store = store.canonicalize()?;
        let _lock = lock(&store)?;
        admit(&store, expected_growth_bytes)?;
        let root = tempfile::Builder::new()
            .prefix("runtime-")
            .tempdir_in(store.join("checkouts"))?
            .keep();
        add_worktree(&store, &root, Path::new(""), &self.source.commit)?;
        for (module, identity) in &self.modules {
            add_worktree(&store, &root.join(module), module, &identity.commit)?;
        }
        let snapshot = Self {
            root,
            ..self.clone()
        };
        snapshot.verify(&store)?;
        Ok(snapshot)
    }

    /// A fixed, store-owned build path keeps Cargo fingerprints stable when the
    /// requested checkout comes from initial setup, an overlay, or a new commit.
    /// It is never the preserved execution root and is locked through sealing.
    pub(crate) fn workspace(&self, store: &Path, expected_growth_bytes: u64) -> Result<Workspace> {
        self.verify(store)?;
        if !self.modules.is_empty() {
            return Err(
                "native mutable build workspace requires this repository's flattened source layout"
                    .into(),
            );
        }
        let store = store.canonicalize()?;
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .custom_flags(
                (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
            )
            .open(store.join("workspace.lock"))?;
        if !file.metadata()?.is_file() {
            return Err("native workspace lock is not regular".into());
        }
        rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive)?;
        let _source_lock = lock(&store)?;
        let root = store.join("build-workspace");
        if fs::symlink_metadata(&root).is_ok() {
            directory(&root)?;
            let common = git([
                OsStr::new("-C"),
                root.as_os_str(),
                OsStr::new("rev-parse"),
                OsStr::new("--path-format=absolute"),
                OsStr::new("--git-common-dir"),
            ])?;
            if Path::new(common.trim_end()).canonicalize()? != repository(&store, Path::new("")) {
                return Err("refusing foreign native build workspace".into());
            }
            source::inspect(&root)?;
            git([
                OsStr::new("-C"),
                root.as_os_str(),
                OsStr::new("checkout"),
                OsStr::new("--detach"),
                OsStr::new(&self.source.commit),
            ])?;
        } else {
            admit(&store, expected_growth_bytes)?;
            add_worktree(&store, &root, Path::new(""), &self.source.commit)?;
        }
        if source::inspect(&root)? != self.source {
            return Err("native build workspace source mismatch".into());
        }
        Ok(Workspace { root, _lock: file })
    }
}

fn verify_update_checkout(
    checkout: &UpdateCheckout,
    snapshot: &Snapshot,
    store: &Path,
) -> Result<()> {
    let root = &checkout.root;
    if checkout.schema != 1
        || checkout.source != snapshot.source
        || root.parent() != Some(store.join("update-checkouts").as_path())
        || root.canonicalize()? != *root
        || !fs::symlink_metadata(root.join(".git"))?.is_dir()
        || source::inspect(root)? != snapshot.source
    {
        return Err("native update checkout is redirected, dirty or has a different source".into());
    }
    let common = git([
        OsStr::new("-C"),
        root.as_os_str(),
        OsStr::new("rev-parse"),
        OsStr::new("--path-format=absolute"),
        OsStr::new("--git-common-dir"),
    ])?;
    let branch = git([
        OsStr::new("-C"),
        root.as_os_str(),
        OsStr::new("rev-parse"),
        OsStr::new("--abbrev-ref"),
        OsStr::new("HEAD"),
    ])?;
    let origin = git([
        OsStr::new("-C"),
        root.as_os_str(),
        OsStr::new("remote"),
        OsStr::new("get-url"),
        OsStr::new("origin"),
    ])?;
    let objects = repository(store, Path::new("")).join("objects");
    if Path::new(common.trim_end()).canonicalize()? != root.join(".git")
        || branch.trim_end() != checkout.branch
        || origin.trim_end() != "https://github.com/bin9208/openpilot-rust.git"
        || fs::read_to_string(root.join(".git/objects/info/alternates"))?.trim_end()
            != objects
                .to_str()
                .ok_or("invalid shared source object path")?
    {
        return Err("native update checkout Git ownership or origin mismatch".into());
    }
    Ok(())
}

fn lock(store: &Path) -> Result<File> {
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
    Ok(lock)
}

fn admit(store: &Path, expected_growth_bytes: u64) -> Result<()> {
    let space = rustix::fs::statvfs(store)?;
    let available = space
        .f_bavail
        .checked_mul(space.f_frsize)
        .ok_or("source space overflow")?;
    if expected_growth_bytes == 0 || available < expected_growth_bytes {
        return Err("insufficient space for this source import; previous sources preserved".into());
    }
    Ok(())
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
    let _lock = lock(&store)?;
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
    admit(&store, expected_growth_bytes)?;
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

#[cfg(test)]
mod native_tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn git(root: &Path, args: &[&str]) {
        let result = Command::new("/usr/bin/git")
            .args([
                "-c",
                "user.name=Native source fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    fn source(parent: &Path) -> PathBuf {
        let root = parent.join("request");
        fs::create_dir(&root).unwrap();
        git(&root, &["init", "--quiet"]);
        fs::write(root.join("tracked.rs"), b"source A").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "--quiet", "-m", "source A"]);
        root
    }

    #[test]
    fn updater_owns_a_complete_git_directory_and_never_resets_dirty_or_previous_source() {
        let temp = tempfile::tempdir().unwrap();
        let input = source(temp.path());
        let store = temp.path().join("store");
        let retained = retain(&input, &store, 8 * 1024 * 1024).unwrap();
        let root = retained
            .update_checkout(&store, "codex/fixture", 8 * 1024 * 1024)
            .unwrap();
        assert!(root.join(".git").is_dir());
        assert!(retained.root.join(".git").is_file());
        fs::remove_dir_all(input).unwrap();
        assert_eq!(
            retained
                .update_checkout(&store, "codex/fixture", 8 * 1024 * 1024)
                .unwrap(),
            root
        );
        fs::write(root.join(".overlay_init"), b"").unwrap();
        assert_eq!(
            retained
                .update_checkout(&store, "codex/fixture", 8 * 1024 * 1024)
                .unwrap(),
            root
        );
        fs::write(root.join("tracked.rs"), b"foreign local edit").unwrap();
        assert!(retained
            .update_checkout(&store, "codex/fixture", 8 * 1024 * 1024)
            .is_err());
        assert_eq!(
            fs::read(root.join("tracked.rs")).unwrap(),
            b"foreign local edit"
        );
        retained.verify(&store).unwrap();
        assert_eq!(
            fs::read(retained.root.join("tracked.rs")).unwrap(),
            b"source A"
        );
    }

    #[test]
    fn fixed_workspace_reuses_its_path_and_cannot_mutate_preserved_sources_or_reset_dirty_input() {
        let temp = tempfile::tempdir().unwrap();
        let root = source(temp.path());
        let store = temp.path().join("store");
        let a = retain(&root, &store, 8 * 1024 * 1024).unwrap();
        let workspace = a.workspace(&store, 8 * 1024 * 1024).unwrap();
        let path = workspace.root.clone();
        assert!(a.workspace(&store, 8 * 1024 * 1024).is_err());
        drop(workspace);
        let running = a.fork(&store, 8 * 1024 * 1024).unwrap();
        fs::write(root.join("tracked.rs"), b"source B").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "--quiet", "-m", "source B"]);
        let b = retain(&root, &store, 8 * 1024 * 1024).unwrap();
        let workspace = b.workspace(&store, 8 * 1024 * 1024).unwrap();
        assert_eq!(workspace.root, path);
        assert_eq!(fs::read(path.join("tracked.rs")).unwrap(), b"source B");
        assert_eq!(
            fs::read(running.root.join("tracked.rs")).unwrap(),
            b"source A"
        );
        fs::write(path.join("tracked.rs"), b"uncommitted workspace input").unwrap();
        drop(workspace);
        assert!(a.workspace(&store, 8 * 1024 * 1024).is_err());
        assert_eq!(
            fs::read(path.join("tracked.rs")).unwrap(),
            b"uncommitted workspace input"
        );
        a.verify(&store).unwrap();
        b.verify(&store).unwrap();
        running.verify(&store).unwrap();
    }

    #[test]
    fn runtime_accepts_only_exact_committed_lfs_objects_and_bound_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let root = source(temp.path());
        let asset = PathBuf::from("openpilot/selfdrive/carrot/xiaoge/assets/lane.onnx");
        fs::create_dir_all(root.join(asset.parent().unwrap())).unwrap();
        let bytes = b"synthetic ONNX object";
        let sha = format!("{:x}", Sha256::digest(bytes));
        fs::write(
            root.join(&asset),
            format!(
                "version https://git-lfs.github.com/spec/v1\noid sha256:{sha}\nsize {}\n",
                bytes.len()
            ),
        )
        .unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "--quiet", "-m", "LFS model identity"]);
        let store = temp.path().join("store");
        let base = retain(&root, &store, 8 * 1024 * 1024).unwrap();
        let runtime = base.fork(&store, 8 * 1024 * 1024).unwrap();
        fs::write(runtime.root.join(&asset), bytes).unwrap();
        let assets = BTreeMap::from([(asset.clone(), sha)]);
        assert!(runtime.verify(&store).is_err());
        runtime.verify_runtime(&store, None, &assets).unwrap();
        let input = crate::generation::SourceForSeal {
            schema: 1,
            source: runtime.clone(),
            source_store: store.clone(),
            assets: assets.clone(),
        };
        let path = temp.path().join("source-for-seal.json");
        fs::write(&path, serde_json::to_vec(&input).unwrap()).unwrap();
        crate::generation::SourceForSeal::read_and_verify(&path, &runtime.root).unwrap();
        fs::write(runtime.root.join("build.json"), b"source A metadata").unwrap();
        let metadata = crate::digest_file(&runtime.root.join("build.json")).unwrap();
        runtime
            .verify_runtime(&store, Some(&metadata), &assets)
            .unwrap();
        fs::write(runtime.root.join("build.json"), b"source B metadata").unwrap();
        assert!(runtime
            .verify_runtime(&store, Some(&metadata), &assets)
            .is_err());
        fs::remove_file(runtime.root.join("build.json")).unwrap();
        fs::write(runtime.root.join(&asset), b"different ONNX value").unwrap();
        let forged = BTreeMap::from([(
            asset.clone(),
            crate::digest_file(&runtime.root.join(&asset)).unwrap(),
        )]);
        assert!(runtime.verify_runtime(&store, None, &forged).is_err());
        fs::write(runtime.root.join(&asset), bytes).unwrap();
        fs::write(
            runtime.root.join("tracked.rs"),
            b"unrelated source mutation",
        )
        .unwrap();
        assert!(runtime.verify_runtime(&store, None, &assets).is_err());
        base.verify(&store).unwrap();
    }
}
