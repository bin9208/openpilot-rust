//! Assemble the pinned native Rust distribution without running install.sh.
//! Every reuse derives its expected tree from authenticated cached archives;
//! a locally rewritten tree receipt cannot certify modified compiler files.
use crate::{
    archive_tree::{directory, file, hash, insert, json, links, normal, parents, sync_directories},
    assets, digest_file,
    inputs::{Entry, Tree},
    regular_file, Result,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Instant,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub asset: assets::Asset,
    pub archive_root: String,
    pub component: String,
    pub maximum_unpacked_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema: u32,
    pub version: String,
    pub target: String,
    pub source_manifest_url: String,
    pub source_manifest_sha256: String,
    pub reserve_bytes: u64,
    pub components: BTreeMap<String, Component>,
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
    pub assets: BTreeMap<String, assets::Receipt>,
    pub copied_bytes: u64,
    pub elapsed_ms: u128,
    pub compiler_executed: bool,
    pub runtime_activated: bool,
}

impl Plan {
    pub fn read(path: &Path) -> Result<Self> {
        let file = regular_file(path)?;
        if file.metadata()?.len() > 65536 {
            return Err("native Rust plan exceeds 64 KiB".into());
        }
        let plan: Self = serde_json::from_reader(file.take(65537))?;
        plan.validate()?;
        Ok(plan)
    }

    fn asset_plan(&self) -> assets::Plan {
        assets::Plan {
            schema: 1,
            assets: self
                .components
                .iter()
                .map(|(name, c)| (name.clone(), c.asset.clone()))
                .collect(),
            reserve_bytes: self.reserve_bytes,
        }
    }

    fn validate(&self) -> Result<()> {
        if self.schema != 1
            || self.version != "1.94.0"
            || self.target != "aarch64-unknown-linux-gnu"
            || self.source_manifest_url
                != "https://static.rust-lang.org/dist/channel-rust-1.94.0.toml"
            || self.source_manifest_sha256.len() != 64
            || !self
                .source_manifest_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self
                .components
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>()
                != ["cargo", "rust-std", "rustc"]
        {
            return Err("unsupported or incomplete native Rust distribution plan".into());
        }
        self.asset_plan().validate()?;
        for (name, component) in &self.components {
            if component.archive_root != format!("{name}-{}-{}", self.version, self.target)
                || component.component
                    != if name == "rust-std" {
                        format!("rust-std-{}", self.target)
                    } else {
                        name.clone()
                    }
                || component.maximum_unpacked_bytes == 0
                || component.maximum_unpacked_bytes > 2 * 1024 * 1024 * 1024
            {
                return Err("invalid native Rust archive layout or unpacked limit".into());
            }
        }
        Ok(())
    }

    fn provenance(&self) -> Result<Vec<u8>> {
        // URLs and space estimates do not alter the installed compiler bytes.
        let components: BTreeMap<_, _> = self
            .components
            .iter()
            .map(|(name, c)| {
                (
                    name,
                    (
                        &c.asset.sha256,
                        c.asset.bytes,
                        &c.archive_root,
                        &c.component,
                    ),
                )
            })
            .collect();
        json(&(
            "openpilot-native-rust-v1",
            &self.version,
            &self.target,
            &self.source_manifest_sha256,
            components,
        ))
    }
}

fn index(
    component: &Component,
    name: &str,
    blob: &Path,
    entries: &mut BTreeMap<PathBuf, Entry>,
    destination: Option<&Path>,
) -> Result<u64> {
    let decoder = xz2::read::XzDecoder::new_stream(
        regular_file(blob)?,
        xz2::stream::Stream::new_stream_decoder(128 * 1024 * 1024, 0)?,
    );
    let decoded_limit = component
        .maximum_unpacked_bytes
        .checked_add(128 * 1024 * 1024)
        .ok_or("native Rust decoded limit overflow")?;
    let mut archive = tar::Archive::new(decoder.take(decoded_limit + 1));
    let root = Path::new(&component.archive_root);
    let selected = root.join(&component.component);
    let mut seen = BTreeSet::new();
    let mut licenses = BTreeSet::new();
    let mut total = 0_u64;
    let mut copied = 0_u64;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        if !normal(&path)
            || path.as_os_str().len() > 4096
            || !path.starts_with(root)
            || !seen.insert(path.clone())
            || seen.len() > 100_000
        {
            return Err("unsafe, foreign or duplicate native Rust archive member".into());
        }
        let size = entry.header().size()?;
        total = total
            .checked_add(size)
            .ok_or("native Rust archive size overflow")?;
        if total > component.maximum_unpacked_bytes {
            return Err("native Rust archive exceeds declared unpacked size".into());
        }
        let kind = entry.header().entry_type();
        let mode = entry.header().mode()?;
        if mode & 0o6000 != 0 || !(kind.is_file() || kind.is_dir() || kind.is_symlink()) {
            return Err("unsupported native Rust archive type or set-id mode".into());
        }
        let relative = if let Ok(path) = path.strip_prefix(&selected) {
            if path.as_os_str().is_empty() {
                continue;
            }
            if path == Path::new("manifest.in") {
                PathBuf::from("provenance").join(name).join("manifest.in")
            } else {
                path.to_owned()
            }
        } else if path.parent() == Some(root)
            && kind.is_file()
            && path
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.starts_with("LICENSE") || s == "COPYRIGHT")
        {
            licenses.insert(
                path.file_name()
                    .ok_or("native Rust license name missing")?
                    .to_owned(),
            );
            PathBuf::from("share/licenses/rust")
                .join(name)
                .join(path.file_name().ok_or("native Rust license name missing")?)
        } else {
            continue;
        };
        if !normal(&relative) {
            return Err("native Rust member escapes component".into());
        }
        let value = if kind.is_dir() {
            if let Some(root) = destination {
                parents(root, &relative.join("placeholder"))?;
            }
            Entry::Directory
        } else if kind.is_symlink() {
            let target = entry
                .link_name()?
                .ok_or("native Rust symlink has no target")?
                .into_owned();
            if target.is_absolute() {
                return Err("absolute native Rust compiler symlink".into());
            }
            Entry::Symlink { target }
        } else {
            let final_mode = if mode & 0o111 != 0 { 0o555 } else { 0o444 };
            let mut output = if let Some(root) = destination {
                parents(root, &relative)?;
                if root.join(&relative).try_exists()? {
                    None
                } else {
                    Some(
                        OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
                            .open(root.join(&relative))?,
                    )
                }
            } else {
                None
            };
            let mut digest = Sha256::new();
            let mut bytes = 0_u64;
            let mut buffer = [0_u8; 65536];
            loop {
                let count = entry.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                bytes = bytes
                    .checked_add(count as u64)
                    .ok_or("native Rust file size overflow")?;
                if bytes > size {
                    return Err("native Rust file exceeds archive size".into());
                }
                digest.update(&buffer[..count]);
                if let Some(output) = &mut output {
                    output.write_all(&buffer[..count])?;
                }
            }
            if bytes != size {
                return Err("truncated native Rust distribution member".into());
            }
            if let Some(output) = output {
                output.set_permissions(fs::Permissions::from_mode(final_mode))?;
                output.sync_all()?;
                copied = copied
                    .checked_add(bytes)
                    .ok_or("native Rust copied bytes overflow")?;
            }
            Entry::File {
                sha256: format!("{:x}", digest.finalize()),
                bytes,
                mode: final_mode,
            }
        };
        insert(entries, relative, value)?;
    }
    // Force the xz footer/check to be consumed even when tar ended at its two
    // zero blocks. Authenticated bytes remain mandatory before this parser.
    let mut decoder = archive.into_inner();
    std::io::copy(&mut decoder, &mut std::io::sink())?;
    if decoder.limit() == 0 {
        return Err("native Rust decoded stream exceeds its bound".into());
    }
    if regular_file(blob)?.metadata()?.len() != component.asset.bytes
        || digest_file(blob)? != component.asset.sha256
    {
        return Err("native Rust archive changed during preparation".into());
    }
    if !licenses.contains(std::ffi::OsStr::new("LICENSE-APACHE"))
        || !licenses.contains(std::ffi::OsStr::new("LICENSE-MIT"))
    {
        return Err("native Rust distribution license files are missing".into());
    }
    Ok(copied)
}

/// Prepare files only. Compilation, native SDK closure and runtime activation
/// remain separate operations and must not be inferred from this receipt.
pub fn prepare(
    plan: &Plan,
    cache: &Path,
    store: &Path,
    download: &impl assets::Download,
) -> Result<Prepared> {
    let started = Instant::now();
    plan.validate()?;
    let provenance = plan.provenance()?;
    let identity = hash(&provenance);
    directory(store)?;
    let store = store.canonicalize()?;
    let object = store.join(&identity);
    directory(&object)?;
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(object.join("lock"))?;
    if !lock.metadata()?.is_file() {
        return Err("native Rust preparation lock is not a regular file".into());
    }
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive)?;
    let generation = object.join("generation");
    let existing = match fs::symlink_metadata(&generation) {
        Ok(metadata) if metadata.is_dir() => true,
        Ok(_) => return Err("native Rust generation is not a real directory".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    if !existing {
        let growth = plan
            .components
            .values()
            .try_fold(plan.reserve_bytes, |n, c| {
                n.checked_add(c.maximum_unpacked_bytes)
                    .and_then(|v| v.checked_add(c.asset.bytes))
                    .ok_or("native Rust growth overflow")
            })?;
        let space = rustix::fs::statvfs(&object)?;
        if space
            .f_bavail
            .checked_mul(space.f_frsize)
            .ok_or("native Rust space overflow")?
            < growth
        {
            return Err("insufficient capacity for declared native Rust unpacking; prior generations retained".into());
        }
    }
    let assets = assets::prepare(cache, &plan.asset_plan(), download)?;
    let temporary = if existing {
        None
    } else {
        Some(
            tempfile::Builder::new()
                .prefix(".pending-")
                .tempdir_in(&object)?,
        )
    };
    let root = temporary.as_ref().map(|p| p.path().join("tree"));
    if let Some(root) = &root {
        directory(root)?;
    }
    let mut entries = BTreeMap::new();
    let mut copied_bytes = 0_u64;
    for (name, component) in &plan.components {
        copied_bytes = copied_bytes
            .checked_add(index(
                component,
                name,
                &assets[name].path,
                &mut entries,
                root.as_deref(),
            )?)
            .ok_or("native Rust copied byte count overflow")?;
    }
    let metadata_path = PathBuf::from("provenance/native-rust.json");
    if let Some(root) = &root {
        file(root, &metadata_path, &provenance, 0o444)?;
        copied_bytes += provenance.len() as u64;
    }
    insert(
        &mut entries,
        metadata_path,
        Entry::File {
            sha256: hash(&provenance),
            bytes: provenance.len() as u64,
            mode: 0o444,
        },
    )?;
    let tree = Tree { schema: 1, entries };
    links(&tree, root.as_deref())?;
    for name in ["bin/rustc", "bin/cargo", "bin/rustdoc"] {
        if !matches!(
            tree.entries.get(Path::new(name)),
            Some(Entry::File { mode: 0o555, .. })
        ) {
            return Err("native Rust distribution lacks required compiler executables".into());
        }
    }
    let standard = PathBuf::from("lib/rustlib").join(&plan.target).join("lib");
    if !tree.entries.iter().any(|(path, entry)| {
        path.parent() == Some(standard.as_path())
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("libstd-") && n.ends_with(".rlib"))
            && matches!(entry, Entry::File { bytes, .. } if *bytes > 0)
    }) {
        return Err("native Rust distribution lacks its target standard library".into());
    }
    let tree_bytes = json(&tree)?;
    if tree_bytes.len() > 16 * 1024 * 1024 {
        return Err("native Rust tree exceeds the existing 16 MiB receipt reader limit".into());
    }
    let installation = Installation {
        schema: 1,
        identity: identity.clone(),
        tree_sha256: hash(&tree_bytes),
    };
    if let Some(temporary) = temporary {
        let root = root.ok_or("missing native Rust staging root")?;
        tree.verify(&root)?;
        for name in ["bin/rustc", "bin/cargo", "bin/rustdoc"] {
            openpilot_runtime_installer::verify_native_elf(&root.join(name), true)?;
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
            return Err(
                "native Rust receipt differs from authenticated distribution archives".into(),
            );
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
