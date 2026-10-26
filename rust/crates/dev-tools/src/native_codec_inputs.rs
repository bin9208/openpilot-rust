//! Content-addressed reuse of the existing pinned ARM codec wheels. No pip,
//! compiler, installer, runtime activation or project binary is involved.
use crate::{
    encoder_stage::{self, Package},
    seal::Result,
};
use openpilot_runtime_build::{
    assets, digest_file,
    inputs::{Entry, Tree},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Cursor, Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Instant,
};

#[derive(Debug, Serialize)]
pub struct Prepared {
    pub schema: u32,
    pub identity: String,
    pub root: PathBuf,
    pub tree_receipt: PathBuf,
    pub tree_sha256: String,
    pub asset: assets::Receipt,
    pub copied_bytes: u64,
    pub elapsed_ms: u128,
    pub compiler_executed: bool,
    pub runtime_activated: bool,
}

pub(crate) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn json(value: &impl Serialize) -> Result<Vec<u8>> {
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
        return Err("native codec store requires real directories".into());
    }
    Ok(())
}
pub(crate) fn regular(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err("native codec input is not a regular file".into());
    }
    Ok(file)
}
pub(crate) fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(path)?;
    file.write_all(bytes)?;
    file.set_permissions(fs::Permissions::from_mode(0o444))?;
    file.sync_all()?;
    Ok(())
}
fn verify_file(path: &Path, expected: &[u8]) -> Result<()> {
    let file = regular(path)?;
    let metadata = file.metadata()?;
    if metadata.len() != expected.len() as u64
        || metadata.permissions().mode() & 0o7777 != 0o444
        || digest_file(path)? != hash(expected)
    {
        return Err(
            "native codec generation metadata differs from its authenticated archive".into(),
        );
    }
    Ok(())
}
fn sync(root: &Path) -> Result<()> {
    let mut dirs = vec![root.to_owned()];
    while let Some(path) = dirs.pop() {
        for child in fs::read_dir(&path)? {
            let child = child?;
            if child.file_type()?.is_dir() {
                dirs.push(child.path());
            }
        }
        File::open(path)?.sync_all()?;
    }
    Ok(())
}
pub(crate) fn insert(
    entries: &mut BTreeMap<PathBuf, Entry>,
    path: PathBuf,
    entry: Entry,
) -> Result<()> {
    let mut parent = path.parent();
    while let Some(p) = parent.filter(|p| !p.as_os_str().is_empty()) {
        if entries.get(p).is_some_and(|e| e != &Entry::Directory) {
            return Err("native codec archive overlaps a non-directory".into());
        }
        entries.insert(p.to_owned(), Entry::Directory);
        parent = p.parent();
    }
    if entries.insert(path, entry).is_some() || entries.len() > 100_000 {
        return Err("native codec archive has duplicate or excessive tree entries".into());
    }
    Ok(())
}

/// Read the exact authenticated bytes, not a receipt from an earlier extraction.
/// Every reuse repeats this pass; expected contents never come from the cache.
fn expected(archive: &Path, package: &Package, destination: Option<&Path>) -> Result<Tree> {
    if package.size > 128 * 1024 * 1024 {
        return Err("native codec archive exceeds 128 MiB".into());
    }
    let mut bytes = Vec::new();
    regular(archive)?
        .take(package.size + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 != package.size || hash(&bytes) != package.sha256 {
        return Err("native codec parser bytes differ from pinned archive".into());
    }
    let mut wheel = zip::ZipArchive::new(Cursor::new(&bytes))?;
    let count =
        encoder_stage::zip_record_count(Cursor::new(&bytes), wheel.central_directory_start())?;
    let selected = encoder_stage::wheel_entries(&mut wheel, package, count)?;
    let mut entries = BTreeMap::new();
    for (index, name, size) in selected {
        let path = PathBuf::from(&name);
        if path.starts_with("provenance") {
            return Err("native codec archive occupies reserved provenance".into());
        }
        let mut entry = wheel.by_index(index)?;
        let mode = if entry.unix_mode().unwrap_or(0) & 0o111 != 0
            || matches!(name.as_str(), "bin/ffmpeg" | "bin/ffprobe")
        {
            0o555
        } else {
            0o444
        };
        let mut output = if let Some(root) = destination {
            let target = root.join(&path);
            fs::create_dir_all(target.parent().ok_or("missing native codec parent")?)?;
            Some(
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
                    .open(target)?,
            )
        } else {
            None
        };
        let mut digest = Sha256::new();
        let mut buffer = [0; 65536];
        let mut copied = 0;
        loop {
            let n = entry.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            copied += n as u64;
            if copied > size {
                return Err("native codec member exceeds ZIP size".into());
            }
            digest.update(&buffer[..n]);
            if let Some(file) = &mut output {
                file.write_all(&buffer[..n])?;
            }
        }
        if copied != size {
            return Err("truncated native codec member".into());
        }
        if let Some(file) = output {
            file.set_permissions(fs::Permissions::from_mode(mode))?;
            file.sync_all()?;
        }
        insert(
            &mut entries,
            path,
            Entry::File {
                sha256: format!("{:x}", digest.finalize()),
                bytes: size,
                mode,
            },
        )?;
    }
    // Preserve original wheel metadata/notices as data, never execute the shim.
    let dist = format!("{}-{}.dist-info/", package.name, package.version);
    let mut metadata_bytes = 0u64;
    let mut metadata_names = BTreeSet::new();
    for index in 0..wheel.len() {
        let mut entry = wheel.by_index(index)?;
        let Some(relative) = entry.name()?.strip_prefix(&dist).map(str::to_owned) else {
            continue;
        };
        if entry.is_dir() {
            continue;
        }
        encoder_stage::safe(&relative)?;
        let mode = entry.unix_mode().unwrap_or(0);
        metadata_bytes = metadata_bytes
            .checked_add(entry.size())
            .ok_or("codec metadata size overflow")?;
        if mode & 0o6000 != 0
            || !matches!(mode & 0o170000, 0 | 0o100000)
            || metadata_bytes > 4 * 1024 * 1024
        {
            return Err("unsupported or oversized native wheel metadata".into());
        }
        let mut bytes = Vec::new();
        let size = entry.size();
        (&mut entry).take(size + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 != size {
            return Err("truncated wheel metadata".into());
        }
        if relative == "METADATA" {
            let text = std::str::from_utf8(&bytes)?;
            let name = format!("Name: {}", package.name);
            let version = format!("Version: {}", package.version);
            if !text.lines().any(|l| l == name) || !text.lines().any(|l| l == version) {
                return Err("native wheel metadata differs from its pinned package".into());
            }
        } else if relative == "WHEEL"
            && !std::str::from_utf8(&bytes)?
                .lines()
                .any(|l| l == "Tag: py3-none-linux_aarch64")
        {
            return Err("native wheel target tag differs from ARM plan".into());
        }
        metadata_names.insert(relative.clone());
        let path = Path::new("provenance/wheel-dist-info").join(relative);
        if let Some(root) = destination {
            fs::create_dir_all(
                root.join(&path)
                    .parent()
                    .ok_or("missing wheel metadata parent")?,
            )?;
            write(&root.join(&path), &bytes)?;
        }
        insert(
            &mut entries,
            path,
            Entry::File {
                sha256: hash(&bytes),
                bytes: size,
                mode: 0o444,
            },
        )?;
    }
    if !["METADATA", "WHEEL", "RECORD"]
        .iter()
        .all(|name| metadata_names.contains(*name))
    {
        return Err("native codec wheel is missing original distribution metadata".into());
    }
    let path = PathBuf::from("provenance/native-codec-wheel.json");
    let provenance = json(&(
        "openpilot-native-codec-wheel-v1",
        "aarch64-unknown-linux-gnu",
        package,
    ))?;
    if let Some(root) = destination {
        directory(&root.join("provenance"))?;
        write(&root.join(&path), &provenance)?;
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
    Ok(Tree { schema: 1, entries })
}

fn prepare_one(
    package: &Package,
    cache: &Path,
    store: &Path,
    download: &impl assets::Download,
) -> Result<Prepared> {
    let identity = hash(&json(&("openpilot-native-codec-wheel-v1", package))?);
    let plan = assets::Plan {
        schema: 1,
        reserve_bytes: 0,
        assets: BTreeMap::from([(
            package.name.clone(),
            assets::Asset {
                url: package.url(),
                sha256: package.sha256.clone(),
                bytes: package.size,
            },
        )]),
    };
    prepare_tree(
        identity,
        store,
        || {
            assets::prepare(cache, &plan, download)?
                .remove(&package.name)
                .ok_or_else(|| "missing native codec asset".into())
        },
        |archive, destination| expected(archive, package, destination),
    )
}

/// Callers derive the expected tree from independently authenticated input bytes
/// on every use. Stored receipts never become the authority for a cache hit.
pub(crate) fn prepare_tree(
    identity: String,
    store: &Path,
    asset: impl FnOnce() -> Result<assets::Receipt>,
    expected: impl Fn(&Path, Option<&Path>) -> Result<Tree>,
) -> Result<Prepared> {
    let started = Instant::now();
    directory(store)?;
    let object = store.canonicalize()?.join(&identity);
    directory(&object)?;
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(object.join("lock"))?;
    if !lock.metadata()?.is_file() {
        return Err("native codec lock is not a regular file".into());
    }
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive)?;
    let asset = asset()?;
    let tree = expected(&asset.path, None)?;
    let tree_bytes = json(&tree)?;
    if tree_bytes.len() > 16 * 1024 * 1024 {
        return Err("native codec tree exceeds receipt limit".into());
    }
    let tree_sha256 = hash(&tree_bytes);
    let record = json(&(1, &identity, &tree_sha256))?;
    let generation = object.join("generation");
    let copied_bytes = match fs::symlink_metadata(&generation) {
        Ok(metadata) if metadata.is_dir() => {
            let names = fs::read_dir(&generation)?
                .map(|e| e.map(|e| e.file_name()))
                .collect::<std::io::Result<BTreeSet<_>>>()?;
            if names
                != ["tree", "tree.json", "receipt.json"]
                    .map(std::ffi::OsString::from)
                    .into_iter()
                    .collect()
            {
                return Err("native codec generation has unexpected metadata members".into());
            }
            verify_file(&generation.join("tree.json"), &tree_bytes)?;
            verify_file(&generation.join("receipt.json"), &record)?;
            tree.verify(&generation.join("tree"))?;
            0
        }
        Ok(_) => return Err("native codec generation is not a real directory".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let payload = tree.entries.values().try_fold(0u64, |total, e| {
                total
                    .checked_add(match e {
                        Entry::File { bytes, .. } => *bytes,
                        _ => 0,
                    })
                    .ok_or("native codec size overflow")
            })?;
            let growth = payload
                .checked_add(tree.entries.len() as u64 * 4096)
                .and_then(|n| n.checked_add(tree_bytes.len() as u64 + record.len() as u64 + 65536))
                .ok_or("native codec growth overflow")?;
            let space = rustix::fs::statvfs(&object)?;
            if space
                .f_bavail
                .checked_mul(space.f_frsize)
                .ok_or("native codec space overflow")?
                < growth
            {
                return Err(
                    "insufficient capacity for native codec prefix; previous inputs retained"
                        .into(),
                );
            }
            let temp = tempfile::Builder::new()
                .prefix(".pending-codec-")
                .tempdir_in(&object)?;
            directory(&temp.path().join("tree"))?;
            if tree != expected(&asset.path, Some(&temp.path().join("tree")))? {
                return Err("native codec archive changed during preparation".into());
            }
            tree.verify(&temp.path().join("tree"))?;
            write(&temp.path().join("tree.json"), &tree_bytes)?;
            write(&temp.path().join("receipt.json"), &record)?;
            sync(temp.path())?;
            rustix::fs::renameat_with(
                rustix::fs::CWD,
                temp.path(),
                rustix::fs::CWD,
                &generation,
                rustix::fs::RenameFlags::NOREPLACE,
            )?;
            File::open(&object)?.sync_all()?;
            payload
        }
        Err(e) => return Err(e.into()),
    };
    if digest_file(&asset.path)? != asset.sha256 {
        return Err("native codec archive changed after preparation".into());
    }
    Ok(Prepared {
        schema: 1,
        identity,
        root: generation.join("tree"),
        tree_receipt: generation.join("tree.json"),
        tree_sha256,
        asset,
        copied_bytes,
        elapsed_ms: started.elapsed().as_millis(),
        compiler_executed: false,
        runtime_activated: false,
    })
}

fn read_lock(root: &Path) -> Result<toml::Value> {
    let file = regular(&root.join("uv.lock"))?;
    if file.metadata()?.len() > 8 * 1024 * 1024 {
        return Err("codec source lock exceeds 8 MiB".into());
    }
    Ok(std::io::read_to_string(file.take(8 * 1024 * 1024 + 1))?.parse()?)
}

pub fn prepare(
    root: &Path,
    cache: &Path,
    store: &Path,
    download: &impl assets::Download,
) -> Result<BTreeMap<String, Prepared>> {
    let lock = read_lock(root)?;
    let packages = encoder_stage::packages();
    for package in &packages {
        encoder_stage::verify_lock(&lock, package)?;
    }
    let mut result = BTreeMap::new();
    for package in &packages {
        result.insert(
            package.name.clone(),
            prepare_one(package, cache, store, download)?,
        );
    }
    let lock = read_lock(root)?;
    for package in &packages {
        encoder_stage::verify_lock(&lock, package)?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Download {
        bytes: BTreeMap<String, Vec<u8>>,
        calls: RefCell<Vec<String>>,
    }
    impl assets::Download for Download {
        fn fetch(&self, asset: &assets::Asset, destination: &Path) -> Result<u64> {
            self.calls.borrow_mut().push(asset.sha256.clone());
            let bytes = &self.bytes[&asset.sha256];
            fs::write(destination, bytes)?;
            Ok(bytes.len() as u64)
        }
    }
    fn wheel(name: &str, payload: &[u8], extra: Option<&str>) -> (Package, Vec<u8>) {
        let mut package = Package {
            name: name.into(),
            version: "1.0".into(),
            commit: "1".repeat(40),
            sha256: String::new(),
            size: 0,
        };
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (path, bytes) in [
            (
                format!("{}include/native.h", package.prefix()),
                payload.to_vec(),
            ),
            (
                format!("{}lib/libnative.a", package.prefix()),
                b"owned library".to_vec(),
            ),
            (
                format!("{name}-1.0.dist-info/METADATA"),
                format!("Name: {name}\nVersion: 1.0\n").into_bytes(),
            ),
            (
                format!("{name}-1.0.dist-info/WHEEL"),
                b"Tag: py3-none-linux_aarch64\n".to_vec(),
            ),
            (
                format!("{name}-1.0.dist-info/RECORD"),
                b"owned original RECORD".to_vec(),
            ),
            (
                format!("{name}-1.0.dist-info/licenses/LICENSE"),
                b"owned vendor notice".to_vec(),
            ),
        ] {
            zip.start_file(path, options).unwrap();
            zip.write_all(&bytes).unwrap();
        }
        if let Some(path) = extra {
            zip.start_file(format!("{}{path}", package.prefix()), options)
                .unwrap();
            zip.write_all(b"unsafe").unwrap();
        }
        let bytes = zip.finish().unwrap().into_inner();
        package.sha256 = hash(&bytes);
        package.size = bytes.len() as u64;
        (package, bytes)
    }
    fn paths(temp: &tempfile::TempDir) -> (PathBuf, PathBuf) {
        (temp.path().join("assets"), temp.path().join("prefixes"))
    }

    #[test]
    fn repeat_and_one_provider_change_retain_unrelated_prefix_and_zero_transfer() {
        let temp = tempfile::tempdir().unwrap();
        let (cache, store) = paths(&temp);
        let (first, a) = wheel("ffmpeg", b"header01", None);
        let (second, b) = wheel("libyuv", b"header02", None);
        let (changed, c) = wheel("ffmpeg", b"header03", None);
        let download = Download {
            bytes: BTreeMap::from([
                (first.sha256.clone(), a),
                (second.sha256.clone(), b),
                (changed.sha256.clone(), c),
            ]),
            calls: RefCell::default(),
        };
        let original = prepare_one(&first, &cache, &store, &download).unwrap();
        let unrelated = prepare_one(&second, &cache, &store, &download).unwrap();
        assert!(original.copied_bytes > 0 && unrelated.copied_bytes > 0);
        let repeat = prepare_one(&first, &cache, &store, &download).unwrap();
        assert_eq!(repeat.root, original.root);
        assert_eq!(repeat.tree_sha256, original.tree_sha256);
        assert_eq!(
            (
                repeat.copied_bytes,
                repeat.asset.transferred_bytes,
                repeat.asset.stored_bytes
            ),
            (0, 0, 0)
        );
        let update = prepare_one(&changed, &cache, &store, &download).unwrap();
        assert_ne!(update.root, original.root);
        let kept = prepare_one(&second, &cache, &store, &download).unwrap();
        assert_eq!(kept.root, unrelated.root);
        assert_eq!((kept.copied_bytes, kept.asset.transferred_bytes), (0, 0));
        assert_eq!(download.calls.borrow().len(), 3);
        Tree::read(&original.tree_receipt)
            .unwrap()
            .verify(&original.root)
            .unwrap();
        assert_eq!(
            fs::read(original.root.join("include/native.h")).unwrap(),
            b"header01"
        );
        assert_eq!(
            fs::read(update.root.join("include/native.h")).unwrap(),
            b"header03"
        );
        assert_eq!(
            fs::read(
                kept.root
                    .join("provenance/wheel-dist-info/licenses/LICENSE")
            )
            .unwrap(),
            b"owned vendor notice"
        );
        assert!(!kept.compiler_executed && !kept.runtime_activated);
    }

    #[test]
    fn forged_extracted_tree_and_both_local_receipts_cannot_replace_vendor_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let (cache, store) = paths(&temp);
        let (package, bytes) = wheel("ffmpeg", b"header01", None);
        let download = Download {
            bytes: BTreeMap::from([(package.sha256.clone(), bytes)]),
            calls: RefCell::default(),
        };
        let original = prepare_one(&package, &cache, &store, &download).unwrap();
        let target = original.root.join("include/native.h");
        fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
        fs::write(&target, b"header02").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o444)).unwrap();
        let forged = json(&Tree::capture(&original.root).unwrap()).unwrap();
        let receipt = original.root.parent().unwrap().join("receipt.json");
        for (path, bytes) in [
            (&original.tree_receipt, forged.clone()),
            (
                &receipt,
                json(&(1, &original.identity, hash(&forged))).unwrap(),
            ),
        ] {
            fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();
            fs::write(path, bytes).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o444)).unwrap();
        }
        assert!(prepare_one(&package, &cache, &store, &download).is_err());
        assert_eq!(download.calls.borrow().len(), 1);
        assert_eq!(fs::read(target).unwrap(), b"header02");
    }

    #[test]
    fn bad_archive_unsafe_paths_and_unknown_metadata_do_not_replace_prior_prefix() {
        let temp = tempfile::tempdir().unwrap();
        let (cache, store) = paths(&temp);
        let (package, bytes) = wheel("libyuv", b"header01", None);
        let mut download = Download {
            bytes: BTreeMap::from([(package.sha256.clone(), bytes)]),
            calls: RefCell::default(),
        };
        let original = prepare_one(&package, &cache, &store, &download).unwrap();
        for path in ["../escape", "provenance/forgery"] {
            let (bad, bytes) = wheel("libyuv", b"header01", Some(path));
            download.bytes.insert(bad.sha256.clone(), bytes);
            assert!(prepare_one(&bad, &cache, &store, &download).is_err());
        }
        let (bad, _) = wheel("libyuv", b"header03", None);
        download
            .bytes
            .insert(bad.sha256.clone(), b"wrong archive".to_vec());
        assert!(prepare_one(&bad, &cache, &store, &download).is_err());
        Tree::read(&original.tree_receipt)
            .unwrap()
            .verify(&original.root)
            .unwrap();
        fs::write(
            original.root.parent().unwrap().join("unexpected"),
            b"foreign metadata",
        )
        .unwrap();
        assert!(prepare_one(&package, &cache, &store, &download).is_err());
        assert!(!temp.path().join("escape").exists());
    }

    #[test]
    fn concurrent_owner_and_generation_symlink_are_rejected_without_following_or_overwrite() {
        let temp = tempfile::tempdir().unwrap();
        let (cache, store) = paths(&temp);
        let (package, bytes) = wheel("libyuv", b"header01", None);
        let download = Download {
            bytes: BTreeMap::from([(package.sha256.clone(), bytes)]),
            calls: RefCell::default(),
        };
        let original = prepare_one(&package, &cache, &store, &download).unwrap();
        let object = original.root.parent().unwrap().parent().unwrap();
        let lock = OpenOptions::new()
            .write(true)
            .open(object.join("lock"))
            .unwrap();
        rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive).unwrap();
        assert!(prepare_one(&package, &cache, &store, &download).is_err());
        drop(lock);
        let saved = object.join("preserved");
        fs::rename(object.join("generation"), &saved).unwrap();
        std::os::unix::fs::symlink(&saved, object.join("generation")).unwrap();
        assert!(prepare_one(&package, &cache, &store, &download).is_err());
        Tree::read(&saved.join("tree.json"))
            .unwrap()
            .verify(&saved.join("tree"))
            .unwrap();
        assert_eq!(download.calls.borrow().len(), 1);
    }
}
