//! Authenticate registry archives against Cargo.lock and prepare directory sources.
//! Package objects are immutable; new lock sets hard-link unchanged source files.
use crate::{
    archive_tree::{directory, file, hash, insert, json, normal, parents, sync_directories},
    assets, digest_file,
    inputs::{Entry, Tree},
    regular_file, Result,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Cursor, Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Instant,
};

const REGISTRY: &str = "registry+https://github.com/rust-lang/crates.io-index";

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub asset: assets::Asset,
    pub unpacked_bytes: u64,
    pub members: u64,
}
impl Package {
    fn root(&self) -> String {
        format!("{}-{}", self.name, self.version)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema: u32,
    pub cargo_lock_sha256: String,
    pub reserve_bytes: u64,
    pub packages: Vec<Package>,
}

#[derive(Deserialize)]
struct Lock {
    package: Vec<LockedPackage>,
}
#[derive(Deserialize)]
struct LockedPackage {
    name: String,
    version: String,
    source: Option<String>,
    checksum: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Prepared {
    pub schema: u32,
    pub identity: String,
    pub root: PathBuf,
    pub vendor: PathBuf,
    pub tree_receipt: PathBuf,
    pub tree_sha256: String,
    pub assets: BTreeMap<String, assets::Receipt>,
    pub copied_bytes: u64,
    pub linked_bytes: u64,
    pub packages: usize,
    pub elapsed_ms: u128,
    pub compiler_executed: bool,
    pub runtime_activated: bool,
}

impl Plan {
    pub fn read(path: &Path) -> Result<Self> {
        let input = regular_file(path)?;
        if input.metadata()?.len() > 4 * 1024 * 1024 {
            return Err("Cargo input plan exceeds 4 MiB".into());
        }
        Ok(serde_json::from_reader(input.take(4 * 1024 * 1024 + 1))?)
    }
    fn assets(&self) -> assets::Plan {
        assets::Plan {
            schema: 1,
            reserve_bytes: self.reserve_bytes,
            assets: self
                .packages
                .iter()
                .map(|p| (p.asset.sha256.clone(), p.asset.clone()))
                .collect(),
        }
    }
    fn validate(&self, lock: &Path) -> Result<()> {
        let mut bytes = Vec::new();
        regular_file(lock)?
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 4 * 1024 * 1024
            || self.schema != 1
            || self.packages.is_empty()
            || self.packages.len() > 1024
            || hash(&bytes) != self.cargo_lock_sha256
        {
            return Err("Cargo input plan differs from its locked source".into());
        }
        let locked: Lock = toml::from_str(std::str::from_utf8(&bytes)?)?;
        let mut expected = BTreeSet::new();
        for p in locked.package {
            if let Some(source) = p.source {
                if source != REGISTRY {
                    return Err(
                        "Cargo input preparation requires explicitly pinned registry sources"
                            .into(),
                    );
                }
                if !expected.insert((
                    p.name,
                    p.version,
                    p.checksum.ok_or("missing locked registry checksum")?,
                )) {
                    return Err("duplicate locked registry package".into());
                }
            }
        }
        let mut actual = BTreeSet::new();
        let mut roots = BTreeSet::new();
        let mut digests = BTreeSet::new();
        for p in &self.packages {
            if p.name.is_empty()
                || !p
                    .name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
                || p.version.is_empty()
                || !p
                    .version
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b".+-".contains(&b))
                || !normal(Path::new(&p.root()))
                || !roots.insert(p.root())
                || !digests.insert(&p.asset.sha256)
                || p.asset.bytes > 64 * 1024 * 1024
                || p.unpacked_bytes == 0
                || p.unpacked_bytes > 512 * 1024 * 1024
                || p.members == 0
                || p.members > 100_000
            {
                return Err("invalid Cargo source package descriptor".into());
            }
            actual.insert((p.name.clone(), p.version.clone(), p.asset.sha256.clone()));
        }
        if actual != expected {
            return Err("Cargo input plan is incomplete or has foreign registry packages".into());
        }
        self.assets().validate()
    }
}

// No caller-selected archive names are used as store paths. Separate content
// locks allow different lock sets to share immutable crate objects safely.
struct Stage {
    object: PathBuf,
    generation: PathBuf,
    root: Option<PathBuf>,
    temporary: Option<tempfile::TempDir>,
    _lock: File,
}
impl Stage {
    fn new(parent: &Path, identity: &str) -> Result<Self> {
        directory(parent)?;
        let object = parent.join(identity);
        directory(&object)?;
        let lock = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
            .open(object.join("lock"))?;
        if !lock.metadata()?.is_file() {
            return Err("Cargo input lock is not regular".into());
        }
        rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive)?;
        let generation = object.join("generation");
        let existing = match fs::symlink_metadata(&generation) {
            Ok(m) if m.is_dir() => true,
            Ok(_) => return Err("Cargo input generation is not a real directory".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => return Err(e.into()),
        };
        let temporary = if existing {
            None
        } else {
            Some(
                tempfile::Builder::new()
                    .prefix(".pending-")
                    .tempdir_in(&object)?,
            )
        };
        let root = temporary.as_ref().map(|t| t.path().join("tree"));
        if let Some(root) = &root {
            directory(root)?;
        }
        Ok(Self {
            object,
            generation,
            root,
            temporary,
            _lock: lock,
        })
    }
    fn finish(self, tree: &Tree) -> Result<(PathBuf, PathBuf, String)> {
        let bytes = json(tree)?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err("Cargo input receipt exceeds 16 MiB".into());
        }
        let expected = hash(&bytes);
        if let Some(temporary) = self.temporary {
            let root = self.root.ok_or("missing Cargo source staging tree")?;
            tree.verify(&root)?;
            file(temporary.path(), Path::new("tree.json"), &bytes, 0o444)?;
            sync_directories(temporary.path())?;
            rustix::fs::renameat_with(
                rustix::fs::CWD,
                temporary.path(),
                rustix::fs::CWD,
                &self.generation,
                rustix::fs::RenameFlags::NOREPLACE,
            )?;
            File::open(&self.object)?.sync_all()?;
        } else {
            if digest_file(&self.generation.join("tree.json"))? != expected {
                return Err("Cargo input receipt differs from authenticated archive bytes".into());
            }
            tree.verify(&self.generation.join("tree"))?;
        }
        Ok((
            self.generation.join("tree"),
            self.generation.join("tree.json"),
            expected,
        ))
    }
}

fn package(p: &Package, blob: &Path, stage: &Stage) -> Result<(Tree, u64)> {
    // Hash the exact bytes fed to the decoder, rather than only checking a path
    // that could have been replaced after the cache admission.
    let mut raw = Vec::new();
    regular_file(blob)?
        .take(p.asset.bytes + 1)
        .read_to_end(&mut raw)?;
    if raw.len() as u64 != p.asset.bytes || hash(&raw) != p.asset.sha256 {
        return Err("Cargo archive parser input differs from its lock checksum".into());
    }
    let decoder = flate2::read::MultiGzDecoder::new(Cursor::new(raw));
    let mut archive = tar::Archive::new(decoder.take(p.unpacked_bytes + 128 * 1024 * 1024 + 1));
    let outer = PathBuf::from(p.root());
    let mut entries = BTreeMap::new();
    let mut checksums = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut total = 0_u64;
    let mut copied = 0;
    for member in archive.entries()? {
        let mut member = member?;
        let path = member.path()?.into_owned();
        if !normal(&path)
            || path.as_os_str().len() > 4096
            || !path.starts_with(&outer)
            || !seen.insert(path.clone())
            || seen.len() > 100_000
            || member.header().mode()? & 0o6000 != 0
        {
            return Err("unsafe, foreign or duplicate Cargo archive member".into());
        }
        let path = path.strip_prefix(&outer)?.to_owned();
        let kind = member.header().entry_type();
        if path.as_os_str().is_empty() && kind.is_dir() {
            continue;
        }
        if path == Path::new(".cargo-checksum.json") || !(kind.is_file() || kind.is_dir()) {
            return Err("unsupported Cargo archive member or prewritten checksum receipt".into());
        }
        if kind.is_dir() {
            if member.size() != 0 {
                return Err("nonempty Cargo archive directory".into());
            }
            if let Some(root) = &stage.root {
                parents(root, &path.join("placeholder"))?;
            }
            insert(&mut entries, path, Entry::Directory)?;
            continue;
        }
        total = total
            .checked_add(member.size())
            .ok_or("Cargo unpacked size overflow")?;
        if total > p.unpacked_bytes {
            return Err("Cargo source exceeds declared unpacked size".into());
        }
        let mode = if member.header().mode()? & 0o111 == 0 {
            0o444
        } else {
            0o555
        };
        let mut output = if let Some(root) = &stage.root {
            parents(root, &path)?;
            Some(
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
                    .open(root.join(&path))?,
            )
        } else {
            None
        };
        let mut digest = Sha256::new();
        let mut count = 0;
        let mut buffer = [0; 65536];
        loop {
            let n = member.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            count += n as u64;
            digest.update(&buffer[..n]);
            if let Some(output) = &mut output {
                output.write_all(&buffer[..n])?;
            }
        }
        if count != member.size() {
            return Err("truncated Cargo source member".into());
        }
        if let Some(output) = output {
            output.set_permissions(fs::Permissions::from_mode(mode))?;
            output.sync_all()?;
            copied += count;
        }
        let sha256 = format!("{:x}", digest.finalize());
        checksums.insert(path.clone(), sha256.clone());
        insert(
            &mut entries,
            path,
            Entry::File {
                sha256,
                bytes: count,
                mode,
            },
        )?;
    }
    let mut decoded = archive.into_inner();
    std::io::copy(&mut decoded, &mut std::io::sink())?;
    if decoded.limit() == 0 || total != p.unpacked_bytes || checksums.len() as u64 != p.members {
        return Err(
            "Cargo archive differs from declared decompression bounds or member inventory".into(),
        );
    }
    if !checksums.contains_key(Path::new("Cargo.toml")) {
        return Err("Cargo package manifest is missing".into());
    }
    let bytes = json(&serde_json::json!({"files":checksums,"package":p.asset.sha256}))?;
    if let Some(root) = &stage.root {
        file(root, Path::new(".cargo-checksum.json"), &bytes, 0o444)?;
        copied += bytes.len() as u64;
    }
    insert(
        &mut entries,
        PathBuf::from(".cargo-checksum.json"),
        Entry::File {
            sha256: hash(&bytes),
            bytes: bytes.len() as u64,
            mode: 0o444,
        },
    )?;
    if digest_file(blob)? != p.asset.sha256 {
        return Err("Cargo archive changed during preparation".into());
    }
    Ok((Tree { schema: 1, entries }, copied))
}

/// Prepare registry sources only. Source helper bootstrap and build/activation
/// remain separate; no project runtime release or installer is used here.
pub fn prepare(
    plan: &Plan,
    lock: &Path,
    cache: &Path,
    store: &Path,
    download: &impl assets::Download,
) -> Result<Prepared> {
    let started = Instant::now();
    plan.validate(lock)?;
    let mut pins: Vec<_> = plan
        .packages
        .iter()
        .map(|p| {
            (
                &p.name,
                &p.version,
                &p.asset.sha256,
                p.asset.bytes,
                p.unpacked_bytes,
                p.members,
            )
        })
        .collect();
    pins.sort();
    // A workspace consumer may enable serde_json/preserve_order. The content
    // identity must not change with its map implementation or feature union.
    let provenance = json(&BTreeMap::from([
        ("schema", serde_json::json!(1)),
        ("format", serde_json::json!("openpilot-native-cargo-v1")),
        (
            "cargo_lock_sha256",
            serde_json::json!(plan.cargo_lock_sha256),
        ),
        ("packages", serde_json::json!(pins)),
    ]))?;
    let identity = hash(&provenance);
    directory(store)?;
    let store = store.canonicalize()?;
    let sources = store.join("crates");
    directory(&sources)?;
    let snapshot = Stage::new(&store.join("sets"), &identity)?;
    if snapshot.root.is_some() {
        // Include raw bodies even when the asset cache happens to share this
        // filesystem; the downloader also checks its own destination capacity.
        let mut growth = plan.packages.iter().try_fold(plan.reserve_bytes, |n, p| {
            n.checked_add(p.asset.bytes)
                .ok_or("Cargo preparation growth overflow")
        })?;
        for p in &plan.packages {
            if !sources
                .join(&p.asset.sha256)
                .join("generation")
                .try_exists()?
            {
                growth = growth
                    .checked_add(p.unpacked_bytes)
                    .and_then(|g| g.checked_add(p.members * 256))
                    .ok_or("Cargo preparation growth overflow")?;
            }
        }
        let space = rustix::fs::statvfs(&store)?;
        if space
            .f_bavail
            .checked_mul(space.f_frsize)
            .ok_or("Cargo preparation space overflow")?
            < growth
        {
            return Err(
                "insufficient capacity for Cargo source preparation; prior inputs retained".into(),
            );
        }
    }
    let assets = assets::prepare(cache, &plan.assets(), download)?;
    let mut entries = BTreeMap::new();
    let mut copied_bytes = 0;
    let mut linked_bytes = 0;
    for p in &plan.packages {
        let object = Stage::new(&sources, &p.asset.sha256)?;
        let (tree, copied) = package(p, &assets[&p.asset.sha256].path, &object)?;
        let (root, _, _) = object.finish(&tree)?;
        copied_bytes += copied;
        let base = PathBuf::from("vendor").join(p.root());
        insert(&mut entries, base.clone(), Entry::Directory)?;
        for (path, entry) in tree.entries {
            let relative = base.join(&path);
            if let Some(destination) = &snapshot.root {
                match &entry {
                    Entry::Directory => parents(destination, &relative.join("placeholder"))?,
                    Entry::File { bytes, .. } => {
                        parents(destination, &relative)?;
                        fs::hard_link(root.join(&path), destination.join(&relative))?;
                        linked_bytes += *bytes;
                    }
                    Entry::Symlink { .. } => return Err("unexpected Cargo source link".into()),
                }
            }
            insert(&mut entries, relative, entry)?;
        }
    }
    let path = PathBuf::from("provenance/native-cargo.json");
    if let Some(root) = &snapshot.root {
        file(root, &path, &provenance, 0o444)?;
        copied_bytes += provenance.len() as u64;
    }
    insert(
        &mut entries,
        path,
        Entry::File {
            sha256: hash(&provenance),
            bytes: provenance.len() as u64,
            mode: 0o444,
        },
    )?;
    let (root, tree_receipt, tree_sha256) = snapshot.finish(&Tree { schema: 1, entries })?;
    plan.validate(lock)?;
    Ok(Prepared {
        schema: 1,
        identity,
        vendor: root.join("vendor"),
        root,
        tree_receipt,
        tree_sha256,
        assets,
        copied_bytes,
        linked_bytes,
        packages: plan.packages.len(),
        elapsed_ms: started.elapsed().as_millis(),
        compiler_executed: false,
        runtime_activated: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tracked_plan_matches_the_complete_current_lock_without_downloads() {
        let rust = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        Plan::read(&rust.join("native-cargo-inputs.json"))
            .unwrap()
            .validate(&rust.join("Cargo.lock"))
            .unwrap();
    }
}
