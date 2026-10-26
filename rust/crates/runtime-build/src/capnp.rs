//! Prepare the pinned Noble ARM Cap'n Proto packages without dpkg or installers.
//! Reuse derives the expected tree from authenticated .deb bytes on every call.
use crate::{
    archive_tree::{directory, file, hash, insert, json, links, parents, sync_directories},
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
    path::{Component, Path, PathBuf},
    time::Instant,
};

const PACKAGES: [(&str, &str); 3] = [
    ("capnproto-1-0-1-4-arm64", "capnproto"),
    ("libcapnp-1-0-1-1-0-1-4-arm64", "libcapnp-1.0.1"),
    ("libcapnp-dev-1-0-1-4-arm64", "libcapnp-dev"),
];
const UNPACKED_LIMIT: u64 = 64 * 1024 * 1024;

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
    pub assets: BTreeMap<String, assets::Receipt>,
    pub copied_bytes: u64,
    pub elapsed_ms: u128,
    pub compiler_executed: bool,
    pub runtime_activated: bool,
}

// Narrowly accept the three short ar member names in the pinned vendor packages.
// No GNU/BSD name tables, arbitrary extraction paths or maintainer execution.
fn ar_member(input: &mut impl Read, expected: &str, limit: u64) -> Result<Vec<u8>> {
    let mut header = [0; 60];
    input.read_exact(&mut header)?;
    let name = std::str::from_utf8(&header[..16])?.trim_end();
    let size: u64 = std::str::from_utf8(&header[48..58])?.trim().parse()?;
    if name.trim_end_matches('/') != expected || &header[58..] != b"`\n" || size > limit {
        return Err("unsupported or oversized native Cap'n Proto deb member".into());
    }
    let mut bytes = vec![0; usize::try_from(size)?];
    input.read_exact(&mut bytes)?;
    if size % 2 != 0 {
        let mut padding = [0];
        input.read_exact(&mut padding)?;
        if padding != *b"\n" {
            return Err("invalid native Cap'n Proto ar padding".into());
        }
    }
    Ok(bytes)
}

fn relative(path: &Path) -> Result<PathBuf> {
    let mut normal = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => (),
            Component::Normal(name) => normal.push(name),
            _ => return Err("native Cap'n Proto archive path escapes prefix".into()),
        }
    }
    if normal.as_os_str().len() > 4096 {
        return Err("native Cap'n Proto archive path is too long".into());
    }
    Ok(normal)
}

fn decoder(bytes: &[u8]) -> Result<impl Read + '_> {
    let mut decoder = zstd::stream::read::Decoder::new(Cursor::new(bytes))?;
    decoder.window_log_max(27)?;
    Ok(decoder)
}

fn control(bytes: &[u8], package: &str) -> Result<Vec<u8>> {
    let mut archive = tar::Archive::new(decoder(bytes)?.take(1024 * 1024 + 1));
    let mut control = None;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = relative(&entry.path()?)?;
        if path == Path::new("control") {
            if control.is_some() || !entry.header().entry_type().is_file() || entry.size() > 65536 {
                return Err("invalid native Cap'n Proto package control member".into());
            }
            let mut value = Vec::new();
            entry.read_to_end(&mut value)?;
            control = Some(value);
        }
    }
    let mut decoded = archive.into_inner();
    std::io::copy(&mut decoded, &mut std::io::sink())?;
    if decoded.limit() == 0 {
        return Err("native Cap'n Proto control exceeds decompression bound".into());
    }
    let control = control.ok_or("missing native Cap'n Proto control")?;
    let mut fields = BTreeMap::new();
    for line in std::str::from_utf8(&control)?.lines() {
        if line.starts_with([' ', '\t']) || line.is_empty() {
            continue;
        }
        let (name, value) = line.split_once(':').ok_or("invalid Debian control field")?;
        if fields.insert(name, value.trim()).is_some() {
            return Err("duplicate Debian control field".into());
        }
    }
    for (name, value) in [
        ("Package", package),
        ("Version", "1.0.1-4"),
        ("Architecture", "arm64"),
    ] {
        if fields.get(name) != Some(&value) {
            return Err("native Cap'n Proto package identity differs from plan".into());
        }
    }
    Ok(control)
}

fn index(
    bytes: &[u8],
    entries: &mut BTreeMap<PathBuf, Entry>,
    destination: Option<&Path>,
    total: &mut u64,
) -> Result<u64> {
    let mut archive =
        tar::Archive::new(decoder(bytes)?.take(UNPACKED_LIMIT + 16 * 1024 * 1024 + 1));
    let mut seen = BTreeSet::new();
    let mut copied = 0;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = relative(&entry.path()?)?;
        let kind = entry.header().entry_type();
        let mode = entry.header().mode()?;
        let size = entry.size();
        if path.as_os_str().is_empty() && kind.is_dir() && size == 0 {
            continue;
        }
        if !path.starts_with("usr")
            || !seen.insert(path.clone())
            || seen.len() > 100_000
            || mode & 0o6000 != 0
            || !(kind.is_dir() || kind.is_file() || kind.is_symlink())
            || (!kind.is_file() && size != 0)
        {
            return Err("unsafe or unsupported native Cap'n Proto data member".into());
        }
        *total = total
            .checked_add(size)
            .ok_or("native Cap'n Proto unpacked size overflow")?;
        if *total > UNPACKED_LIMIT {
            return Err("native Cap'n Proto payload exceeds 64 MiB unpacked bound".into());
        }
        let value = if kind.is_dir() {
            if let Some(root) = destination {
                parents(root, &path.join("placeholder"))?;
            }
            Entry::Directory
        } else if kind.is_symlink() {
            let target = entry
                .link_name()?
                .ok_or("missing native Cap'n Proto link target")?
                .into_owned();
            if target.is_absolute() {
                return Err("absolute native Cap'n Proto link".into());
            }
            Entry::Symlink { target }
        } else {
            let mode = if mode & 0o111 == 0 { 0o444 } else { 0o555 };
            let mut output = if let Some(root) = destination {
                parents(root, &path)?;
                if fs::symlink_metadata(root.join(&path)).is_ok() {
                    None
                } else {
                    Some(
                        OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
                            .open(root.join(&path))?,
                    )
                }
            } else {
                None
            };
            let mut digest = Sha256::new();
            let mut count = 0;
            let mut buffer = [0; 65536];
            loop {
                let n = entry.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                count += n as u64;
                if count > size {
                    return Err("native Cap'n Proto member exceeds its size".into());
                }
                digest.update(&buffer[..n]);
                if let Some(output) = &mut output {
                    output.write_all(&buffer[..n])?;
                }
            }
            if count != size {
                return Err("truncated native Cap'n Proto data member".into());
            }
            if let Some(output) = output {
                output.set_permissions(fs::Permissions::from_mode(mode))?;
                output.sync_all()?;
                copied += size;
            }
            Entry::File {
                sha256: format!("{:x}", digest.finalize()),
                bytes: count,
                mode,
            }
        };
        insert(entries, path, value)?;
    }
    let mut decoded = archive.into_inner();
    std::io::copy(&mut decoded, &mut std::io::sink())?;
    if decoded.limit() == 0 {
        return Err("native Cap'n Proto data exceeds decompression bound".into());
    }
    Ok(copied)
}

/// Prepare an immutable private SDK prefix; this does not compile or install it.
pub fn prepare(
    plan: &assets::Plan,
    cache: &Path,
    store: &Path,
    download: &impl assets::Download,
) -> Result<Prepared> {
    let started = Instant::now();
    plan.validate()?;
    if plan.assets.keys().map(String::as_str).collect::<Vec<_>>() != PACKAGES.map(|(name, _)| name)
        || plan.assets.values().any(|a| a.bytes > 16 * 1024 * 1024)
    {
        return Err("unsupported or incomplete native Cap'n Proto package plan".into());
    }
    let pins: BTreeMap<_, _> = plan
        .assets
        .iter()
        .map(|(name, a)| (name, (&a.sha256, a.bytes)))
        .collect();
    let provenance = json(&("openpilot-native-capnp-deb-v1", "1.0.1-4", "arm64", pins))?;
    let identity = hash(&provenance);
    directory(store)?;
    let object = store.canonicalize()?.join(&identity);
    directory(&object)?;
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(object.join("lock"))?;
    if !lock.metadata()?.is_file() {
        return Err("native Cap'n Proto lock is not a regular file".into());
    }
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive)?;
    let generation = object.join("generation");
    let existing = match fs::symlink_metadata(&generation) {
        Ok(m) if m.is_dir() => true,
        Ok(_) => return Err("native Cap'n Proto generation is not a real directory".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(e.into()),
    };
    if !existing {
        let growth = plan
            .assets
            .values()
            .try_fold(UNPACKED_LIMIT, |n, a| {
                n.checked_add(a.bytes)
                    .ok_or("native Cap'n Proto growth overflow")
            })?
            .checked_add(plan.reserve_bytes)
            .ok_or("native Cap'n Proto growth overflow")?;
        let space = rustix::fs::statvfs(&object)?;
        if space
            .f_bavail
            .checked_mul(space.f_frsize)
            .ok_or("native Cap'n Proto space overflow")?
            < growth
        {
            return Err("insufficient capacity for native Cap'n Proto preparation; prior generations retained".into());
        }
    }
    let assets = assets::prepare(cache, plan, download)?;
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
    let mut entries = BTreeMap::new();
    let mut copied_bytes = 0;
    let mut total = 0;
    for (name, package) in PACKAGES {
        let path = &assets[name].path;
        // Authenticate the exact bounded bytes parsed below, independently of a
        // later pathname recheck. Replacing a cached path during parsing cannot
        // make a different descriptor's contents become an expected SDK tree.
        let mut bytes = Vec::new();
        regular_file(path)?
            .take(plan.assets[name].bytes + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != plan.assets[name].bytes || hash(&bytes) != plan.assets[name].sha256
        {
            return Err("native Cap'n Proto parser input differs from authenticated asset".into());
        }
        let mut input = Cursor::new(bytes);
        let mut magic = [0; 8];
        input.read_exact(&mut magic)?;
        if &magic != b"!<arch>\n"
            || ar_member(&mut input, "debian-binary", 4)?.as_slice() != b"2.0\n"
        {
            return Err("unsupported native Cap'n Proto Debian archive".into());
        }
        let control = control(
            &ar_member(&mut input, "control.tar.zst", 1024 * 1024)?,
            package,
        )?;
        copied_bytes += index(
            &ar_member(&mut input, "data.tar.zst", 16 * 1024 * 1024)?,
            &mut entries,
            root.as_deref(),
            &mut total,
        )?;
        if input.read(&mut [0; 1])? != 0 || digest_file(path)? != plan.assets[name].sha256 {
            return Err("native Cap'n Proto archive changed or has extra members".into());
        }
        let path = Path::new("provenance").join(package).join("control");
        if let Some(root) = &root {
            file(root, &path, &control, 0o444)?;
            copied_bytes += control.len() as u64;
        }
        insert(
            &mut entries,
            path,
            Entry::File {
                sha256: hash(&control),
                bytes: control.len() as u64,
                mode: 0o444,
            },
        )?;
        if !matches!(entries.get(&Path::new("usr/share/doc").join(package).join("copyright")), Some(Entry::File { bytes, .. }) if *bytes > 0)
        {
            return Err("native Cap'n Proto package copyright is missing".into());
        }
    }
    let path = PathBuf::from("provenance/native-capnp.json");
    if let Some(root) = &root {
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
    let tree = Tree { schema: 1, entries };
    links(&tree, root.as_deref())?;
    for path in [
        "usr/bin/capnp",
        "usr/bin/capnpc-c++",
        "usr/bin/capnpc-capnp",
    ] {
        if !matches!(
            tree.entries.get(Path::new(path)),
            Some(Entry::File { mode: 0o555, .. })
        ) {
            return Err("native Cap'n Proto compiler executable is missing".into());
        }
    }
    for path in [
        "usr/include/capnp/message.h",
        "usr/lib/aarch64-linux-gnu/libcapnp-1.0.1.so",
        "usr/lib/aarch64-linux-gnu/libkj-1.0.1.so",
    ] {
        if !matches!(tree.entries.get(Path::new(path)), Some(Entry::File { bytes, .. }) if *bytes > 0)
        {
            return Err("native Cap'n Proto SDK header or library is missing".into());
        }
    }
    let tree_bytes = json(&tree)?;
    if tree_bytes.len() > 16 * 1024 * 1024 {
        return Err("native Cap'n Proto tree receipt exceeds 16 MiB".into());
    }
    let installation = Installation {
        schema: 1,
        identity: identity.clone(),
        tree_sha256: hash(&tree_bytes),
    };
    if let Some(temporary) = temporary {
        let root = root.ok_or("missing native Cap'n Proto staging root")?;
        tree.verify(&root)?;
        for name in [
            "usr/bin/capnp",
            "usr/bin/capnpc-c++",
            "usr/bin/capnpc-capnp",
        ] {
            openpilot_runtime_installer::verify_native_elf(&root.join(name), true)?;
        }
        for name in ["libcapnp-1.0.1.so", "libkj-1.0.1.so"] {
            openpilot_runtime_installer::verify_native_elf(
                &root.join("usr/lib/aarch64-linux-gnu").join(name),
                true,
            )?;
        }
        file(temporary.path(), Path::new("tree.json"), &tree_bytes, 0o444)?;
        file(
            temporary.path(),
            Path::new("receipt.json"),
            &json(&installation)?,
            0o444,
        )?;
        sync_directories(temporary.path())?;
        rustix::fs::renameat_with(
            rustix::fs::CWD,
            temporary.path(),
            rustix::fs::CWD,
            &generation,
            rustix::fs::RenameFlags::NOREPLACE,
        )?;
        File::open(&object)?.sync_all()?;
    } else {
        let stored: Installation =
            serde_json::from_reader(regular_file(&generation.join("receipt.json"))?.take(65537))?;
        if stored != installation
            || digest_file(&generation.join("tree.json"))? != installation.tree_sha256
        {
            return Err("native Cap'n Proto receipt differs from authenticated archives".into());
        }
        tree.verify(&generation.join("tree"))?;
    }
    Ok(Prepared {
        schema: 1,
        identity,
        root: generation.join("tree"),
        tree_receipt: generation.join("tree.json"),
        tree_sha256: installation.tree_sha256,
        assets,
        copied_bytes,
        elapsed_ms: started.elapsed().as_millis(),
        compiler_executed: false,
        runtime_activated: false,
    })
}
