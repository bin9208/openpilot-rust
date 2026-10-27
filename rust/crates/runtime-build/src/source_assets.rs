//! Source LFS bytes are admitted by the current committed OID/size, never by an
//! old runtime's source tag. Preparation does not hydrate or mutate a checkout.
use crate::{
    archive_tree::{directory, file, hash, insert, json, parents, sync_directories},
    assets, digest_file,
    inputs::{Entry, Tree},
    objects, regular_file, source, Result, Source,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Component, Path, PathBuf},
    time::Instant,
};

const METADATA_GROWTH: u64 = 8 * 1024 * 1024;
const PROVENANCE: &str = "provenance/committed-source-lfs.json";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema: u32,
    /// HTTPS locator anchor, kept separate from the current consuming source.
    pub origin_commit: String,
    pub assets: BTreeMap<PathBuf, assets::Asset>,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct Installation {
    schema: u32,
    identity: String,
    tree_sha256: String,
}

#[derive(Debug, Serialize)]
pub struct Prepared {
    pub schema: u32,
    pub identity: String,
    pub root: PathBuf,
    pub tree_receipt: PathBuf,
    pub tree_sha256: String,
    pub source: Source,
    pub origin_commit: String,
    pub assets: BTreeMap<PathBuf, assets::Receipt>,
    pub asset_cache_consulted: bool,
    pub copied_bytes: u64,
    /// Logical bytes hardlinked from verified store-owned objects, not disk growth.
    pub linked_bytes: u64,
    pub expected_additional_bytes: u64,
    pub elapsed_ms: u128,
    pub compiler_executed: bool,
    pub checkout_modified: bool,
    pub runtime_activated: bool,
}

fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Plan {
    pub fn read(path: &Path) -> Result<Self> {
        let input = regular_file(path)?;
        if input.metadata()?.len() > 1024 * 1024 {
            return Err("source LFS plan exceeds 1 MiB".into());
        }
        let plan: Self = serde_json::from_reader(input.take(1024 * 1024 + 1))?;
        plan.validate()?;
        Ok(plan)
    }

    fn validate(&self) -> Result<()> {
        if self.schema != 1
            || !hex(&self.origin_commit, 40)
            || self.assets.is_empty()
            || self.assets.len() > 17
        {
            return Err("invalid committed source LFS plan".into());
        }
        let mut total = 0_u64;
        let mut unique = BTreeMap::new();
        for (path, asset) in &self.assets {
            if !openpilot_runtime_installer::source_asset_allowed(path)
                || !path.components().all(|c| matches!(c, Component::Normal(_)))
                || !hex(&asset.sha256, 64)
                || asset.bytes == 0
                || asset.url
                    != format!(
                        "https://media.githubusercontent.com/media/bin9208/openpilot-rust/{}/{}",
                        self.origin_commit,
                        path.to_str().ok_or("non-UTF8 source LFS path")?
                    )
            {
                return Err("source LFS locator/path/object differs from its declared plan".into());
            }
            total = total
                .checked_add(asset.bytes)
                .ok_or("source LFS size overflow")?;
            if unique
                .insert(&asset.sha256, asset.bytes)
                .is_some_and(|previous| previous != asset.bytes)
            {
                return Err("conflicting sizes for the same source LFS object".into());
            }
        }
        if total > 128 * 1024 * 1024 {
            return Err("source LFS payload exceeds source activation bounds".into());
        }
        Ok(())
    }

    pub fn verify_source(&self, root: &Path) -> Result<Source> {
        self.validate()?;
        let before = source::inspect(root)?;
        let expected = crate::source_lfs::identities(root)?;
        let declared: BTreeMap<_, _> = self
            .assets
            .iter()
            .map(|(path, asset)| (path.clone(), (asset.sha256.clone(), asset.bytes)))
            .collect();
        if expected != declared || source::inspect(root)? != before {
            return Err(
                "source LFS plan does not match every current committed pointer or source changed"
                    .into(),
            );
        }
        Ok(before)
    }
}

fn outside(root: &Path, path: &Path) -> Result<PathBuf> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, Component::CurDir | Component::ParentDir))
    {
        return Err("source LFS stores require absolute contained path spellings".into());
    }
    let path = path
        .parent()
        .ok_or("source LFS store needs a parent")?
        .canonicalize()?
        .join(path.file_name().ok_or("source LFS store needs a name")?);
    if path.starts_with(root) || root.starts_with(&path) || path.is_symlink() {
        return Err("source LFS caches/stores must be outside the source checkout".into());
    }
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_dir() => (),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Ok(_) => return Err("source LFS store path is not a real directory".into()),
        Err(error) => return Err(error.into()),
    }
    Ok(path)
}

fn require_source(plan: &Plan, root: &Path, expected: &Source) -> Result<()> {
    if plan.verify_source(root)? != *expected {
        return Err("source changed during LFS preparation".into());
    }
    Ok(())
}

fn have_object(store: &Path, expected: &objects::Object) -> Result<bool> {
    let directory = store.join(format!("{}-444", expected.sha256));
    match fs::symlink_metadata(&directory) {
        Ok(metadata) if metadata.is_dir() => (),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Ok(_) => return Err("source LFS immutable object directory is redirected".into()),
        Err(error) => return Err(error.into()),
    }
    match fs::symlink_metadata(directory.join("blob")) {
        // A canceled import may have created only its directory/lock. It has
        // not published a content object and can be retried normally.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Ok(_) => {
            expected.verify(store)?;
            Ok(true)
        }
        Err(error) => Err(error.into()),
    }
}

pub fn prepare(
    plan: &Plan,
    source_root: &Path,
    cache: &Path,
    store: &Path,
    object_store: &Path,
    download: &impl assets::Download,
) -> Result<Prepared> {
    let started = Instant::now();
    let source_root = source_root.canonicalize()?;
    let source = plan.verify_source(&source_root)?;
    let cache = outside(&source_root, cache)?;
    let store = outside(&source_root, store)?;
    let object_store = outside(&source_root, object_store)?;
    let paths = [&cache, &store, &object_store];
    for (index, first) in paths.iter().enumerate() {
        for second in &paths[index + 1..] {
            if first.starts_with(second) || second.starts_with(first) {
                return Err("source LFS cache, prefix and object stores must be disjoint".into());
            }
        }
    }
    let provenance = json(&("openpilot-committed-lfs-inputs-v1", plan))?;
    let identity = hash(&provenance);
    let mut entries = BTreeMap::new();
    for (path, asset) in &plan.assets {
        insert(
            &mut entries,
            path.clone(),
            Entry::File {
                sha256: asset.sha256.clone(),
                bytes: asset.bytes,
                mode: 0o444,
            },
        )?;
    }
    insert(
        &mut entries,
        PROVENANCE.into(),
        Entry::File {
            sha256: hash(&provenance),
            bytes: provenance.len() as u64,
            mode: 0o444,
        },
    )?;
    let tree = Tree { schema: 1, entries };
    let tree_bytes = json(&tree)?;
    let installation = Installation {
        schema: 1,
        identity: identity.clone(),
        tree_sha256: hash(&tree_bytes),
    };
    directory(&store)?;
    let object = store.join(&identity);
    directory(&object)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(object.join("lock"))?;
    if !lock.metadata()?.is_file() {
        return Err("source LFS prefix lock is not a regular file".into());
    }
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive)?;
    let generation = object.join("generation");
    let root = generation.join("tree");
    let mut assets = BTreeMap::new();
    let mut copied_bytes = 0;
    let mut linked_bytes = 0;
    let mut expected_additional_bytes = 0;
    let mut asset_cache_consulted = false;
    match fs::symlink_metadata(&generation) {
        Ok(metadata) if metadata.is_dir() => {
            let receipt = regular_file(&generation.join("receipt.json"))?;
            if receipt.metadata()?.len() > 65536
                || regular_file(&generation.join("tree.json"))?
                    .metadata()?
                    .len()
                    != tree_bytes.len() as u64
            {
                return Err("source LFS installation metadata exceeds its expected bounds".into());
            }
            let stored: Installation = serde_json::from_reader(receipt.take(65537))?;
            if stored != installation
                || digest_file(&generation.join("tree.json"))? != installation.tree_sha256
            {
                return Err(
                    "source LFS prefix receipt differs from current committed objects".into(),
                );
            }
            tree.verify(&root)?;
            for (path, asset) in &plan.assets {
                assets.insert(
                    path.clone(),
                    assets::Receipt {
                        schema: 1,
                        sha256: asset.sha256.clone(),
                        path: root.join(path),
                        bytes: asset.bytes,
                        transferred_bytes: 0,
                        stored_bytes: 0,
                        elapsed_ms: 0,
                    },
                );
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            directory(&object_store)?;
            if fs::metadata(&object_store)?.dev() != fs::metadata(&object)?.dev() {
                return Err("source LFS prefix and immutable objects require the same filesystem for hardlinks".into());
            }
            let mut objects_by_sha = BTreeMap::new();
            let mut missing = BTreeMap::new();
            let mut raw_growth = 0_u64;
            let mut object_growth = 0_u64;
            for asset in plan.assets.values() {
                let std::collections::btree_map::Entry::Vacant(entry) =
                    objects_by_sha.entry(asset.sha256.clone())
                else {
                    continue;
                };
                let expected = objects::Object {
                    sha256: asset.sha256.clone(),
                    bytes: asset.bytes,
                    mode: 0o444,
                };
                if !have_object(&object_store, &expected)? {
                    object_growth = object_growth
                        .checked_add(asset.bytes)
                        .ok_or("source LFS object growth overflow")?;
                    let blob = cache.join("sha256").join(&asset.sha256).join("blob");
                    match fs::symlink_metadata(blob) {
                        Ok(_) => (), // assets::prepare authenticates any consumed hit before import.
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            raw_growth = raw_growth
                                .checked_add(asset.bytes)
                                .ok_or("source LFS raw growth overflow")?;
                        }
                        Err(error) => return Err(error.into()),
                    }
                    missing.insert(format!("source-lfs-{}", asset.sha256), asset.clone());
                }
                entry.insert(expected);
            }
            expected_additional_bytes = raw_growth
                .checked_add(object_growth)
                .and_then(|n| n.checked_add(METADATA_GROWTH))
                .ok_or("source LFS preparation growth overflow")?;
            directory(&cache)?;
            for path in [&cache, &object_store, &object] {
                let space = rustix::fs::statvfs(path)?;
                if space
                    .f_bavail
                    .checked_mul(space.f_frsize)
                    .ok_or("source LFS space overflow")?
                    < expected_additional_bytes
                {
                    return Err("insufficient capacity for this source LFS operation; prior inputs retained".into());
                }
            }
            asset_cache_consulted = !missing.is_empty();
            let fetched = if missing.is_empty() {
                BTreeMap::new()
            } else {
                assets::prepare(
                    &cache,
                    &assets::Plan {
                        schema: 1,
                        assets: missing,
                        reserve_bytes: object_growth + METADATA_GROWTH,
                    },
                    download,
                )?
            };
            for expected in objects_by_sha.values() {
                if let Some(raw) = fetched.get(&format!("source-lfs-{}", expected.sha256)) {
                    let imported = objects::import(&object_store, &raw.path, METADATA_GROWTH)?;
                    if imported.object != *expected {
                        return Err("source LFS object changed during immutable import".into());
                    }
                    copied_bytes += imported.stored_bytes;
                }
            }
            let temporary = tempfile::Builder::new()
                .prefix(".pending-")
                .tempdir_in(&object)?;
            let staged = temporary.path().join("tree");
            directory(&staged)?;
            let mut accounted = BTreeSet::new();
            for (path, asset) in &plan.assets {
                let content = &objects_by_sha[&asset.sha256];
                parents(&staged, path)?;
                content.materialize(&object_store, &staged.join(path))?;
                linked_bytes += asset.bytes;
                let fetched = fetched
                    .get(&format!("source-lfs-{}", asset.sha256))
                    .filter(|_| accounted.insert(asset.sha256.clone()));
                assets.insert(
                    path.clone(),
                    assets::Receipt {
                        schema: 1,
                        sha256: asset.sha256.clone(),
                        path: root.join(path),
                        bytes: asset.bytes,
                        transferred_bytes: fetched.map_or(0, |row| row.transferred_bytes),
                        stored_bytes: fetched.map_or(0, |row| row.stored_bytes),
                        elapsed_ms: fetched.map_or(0, |row| row.elapsed_ms),
                    },
                );
            }
            file(&staged, Path::new(PROVENANCE), &provenance, 0o444)?;
            copied_bytes += provenance.len() as u64;
            tree.verify(&staged)?;
            file(temporary.path(), Path::new("tree.json"), &tree_bytes, 0o444)?;
            file(
                temporary.path(),
                Path::new("receipt.json"),
                &json(&installation)?,
                0o444,
            )?;
            sync_directories(temporary.path())?;
            require_source(plan, &source_root, &source)?;
            rustix::fs::renameat_with(
                rustix::fs::CWD,
                temporary.path(),
                rustix::fs::CWD,
                &generation,
                rustix::fs::RenameFlags::NOREPLACE,
            )?;
            File::open(&object)?.sync_all()?;
        }
        Ok(_) => return Err("source LFS generation is not a real directory".into()),
        Err(error) => return Err(error.into()),
    }
    require_source(plan, &source_root, &source)?;
    Ok(Prepared {
        schema: 1,
        identity,
        root,
        tree_receipt: generation.join("tree.json"),
        tree_sha256: installation.tree_sha256,
        source,
        origin_commit: plan.origin_commit.clone(),
        assets,
        asset_cache_consulted,
        copied_bytes,
        linked_bytes,
        expected_additional_bytes,
        elapsed_ms: started.elapsed().as_millis(),
        compiler_executed: false,
        checkout_modified: false,
        runtime_activated: false,
    })
}
