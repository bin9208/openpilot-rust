//! Stage the retained source inputs for the soundd checker in an absent root.
//!
//! This replaces one CI staging heredoc only. It does not replace soundd's
//! checker, policy, native fixture, or any build step.
use crate::seal::Result;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Component, Path, PathBuf},
};

const MANIFEST: &[u8] = include_bytes!("../tests/fixtures/soundd-daemon/manifest.json");
const MANIFEST_SHA256: &str = "14dcbee0272317c38d8a804ec5dc47abcd353710cab0d1dbda6fc0417ae82d82";
const RETIRED: &str = "rust/tools/check_soundd_daemon.py";
const RETIRED_SHA256: &str = "fadc1ad42b382d4e4a6be128ead810e2b694e69b937525cb280e8f59cd799664";
const EXPECTED_SOURCES: usize = 21;
const COPIED_SOURCES: usize = EXPECTED_SOURCES - 1;
const MAX_SOURCE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct StageOptions {
    pub root: PathBuf,
    pub output: PathBuf,
}

struct HeldSource {
    name: String,
    path: PathBuf,
    file: File,
    dev: u64,
    ino: u64,
    len: u64,
}

fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into().into())
    }
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn relative(name: &str) -> Result<&Path> {
    let path = Path::new(name);
    require(
        !name.is_empty()
            && !name.contains('\\')
            && !name.contains('\0')
            && path
                .components()
                .all(|component| matches!(component, Component::Normal(_))),
        format!("unsafe soundd source path: {name}"),
    )?;
    Ok(path)
}

fn source_pins() -> Result<BTreeMap<String, String>> {
    require(
        sha(MANIFEST) == MANIFEST_SHA256,
        "soundd source manifest changed",
    )?;
    let manifest: Value = serde_json::from_slice(MANIFEST)?;
    let object = manifest["source_sha256"]
        .as_object()
        .ok_or("soundd manifest has no source_sha256 map")?;
    require(
        object.len() == EXPECTED_SOURCES,
        "soundd source manifest count changed",
    )?;
    let mut pins = BTreeMap::new();
    for (name, digest) in object {
        relative(name)?;
        let digest = digest
            .as_str()
            .ok_or("soundd source digest is not a string")?;
        require(
            digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()),
            format!("invalid soundd source digest: {name}"),
        )?;
        pins.insert(name.clone(), digest.to_owned());
    }
    require(
        pins.get(RETIRED)
            .is_some_and(|digest| digest == RETIRED_SHA256),
        "soundd historical checker pin changed",
    )?;
    Ok(pins)
}

fn open_source(root: &Path, name: &str, expected: &str) -> Result<HeldSource> {
    let relative = relative(name)?;
    let path = root.join(relative);
    for parent in path.ancestors().skip(1) {
        let metadata = fs::symlink_metadata(parent)?;
        require(
            metadata.file_type().is_dir(),
            format!(
                "soundd source parent is not a real directory: {}",
                parent.display()
            ),
        )?;
    }
    let file = File::from(rustix::fs::open(
        &path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?);
    let metadata = file.metadata()?;
    require(
        metadata.is_file() && metadata.len() <= MAX_SOURCE_BYTES,
        format!("soundd source is nonregular or oversized: {name}"),
    )?;
    let source = HeldSource {
        name: name.to_owned(),
        path,
        file,
        dev: metadata.dev(),
        ino: metadata.ino(),
        len: metadata.len(),
    };
    let digest = hash_held(&source)?;
    require(
        digest == expected,
        format!("soundd source hash changed: {name}"),
    )?;
    Ok(source)
}

fn hash_held(source: &HeldSource) -> Result<String> {
    let mut file = source.file.try_clone()?;
    file.seek(SeekFrom::Start(0))?;
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let count = file.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .ok_or("soundd source size overflow")?;
        require(
            total <= MAX_SOURCE_BYTES,
            "soundd source exceeds byte bound",
        )?;
        hash.update(&bytes[..count]);
    }
    require(
        total == source.len,
        "soundd source length changed while reading",
    )?;
    Ok(format!("{:x}", hash.finalize()))
}

fn verify_held(source: &HeldSource, expected: &str) -> Result<()> {
    let held = source.file.metadata()?;
    require(
        held.is_file()
            && (held.dev(), held.ino(), held.len()) == (source.dev, source.ino, source.len),
        format!("held soundd source identity changed: {}", source.name),
    )?;
    require(
        hash_held(source)? == expected,
        format!("held soundd source changed during staging: {}", source.name),
    )?;
    let current = open_source_identity(&source.path)?;
    let current_metadata = current.metadata()?;
    require(
        (
            current_metadata.dev(),
            current_metadata.ino(),
            current_metadata.len(),
        ) == (source.dev, source.ino, source.len),
        format!(
            "soundd source path was replaced during staging: {}",
            source.name
        ),
    )?;
    let mut current_file = current;
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let count = current_file.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        require(
            total <= MAX_SOURCE_BYTES,
            "soundd source exceeds byte bound",
        )?;
        hash.update(&bytes[..count]);
    }
    require(
        total == source.len && format!("{:x}", hash.finalize()) == expected,
        format!(
            "soundd source content changed during staging: {}",
            source.name
        ),
    )
}

fn open_source_identity(path: &Path) -> Result<File> {
    Ok(File::from(rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?))
}

fn copy_source(source: &mut HeldSource, destination: &Path, expected: &str) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o666)
        .open(destination)?;
    source.file.seek(SeekFrom::Start(0))?;
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let count = source.file.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .ok_or("soundd staged source size overflow")?;
        require(
            total <= MAX_SOURCE_BYTES,
            "soundd source exceeds byte bound",
        )?;
        output.write_all(&bytes[..count])?;
        hash.update(&bytes[..count]);
    }
    output.sync_all()?;
    require(
        total == source.len && format!("{:x}", hash.finalize()) == expected,
        format!("soundd source changed while copying: {}", source.name),
    )
}

fn staged_roster(root: &Path) -> Result<BTreeSet<String>> {
    fn visit(root: &Path, current: &Path, files: &mut BTreeSet<String>) -> Result<()> {
        for entry in fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_dir() {
                visit(root, &path, files)?;
            } else {
                require(
                    metadata.file_type().is_file(),
                    format!(
                        "unexpected nonregular staged soundd path: {}",
                        path.display()
                    ),
                )?;
                let name = path
                    .strip_prefix(root)?
                    .to_str()
                    .ok_or("staged soundd path is not UTF-8")?
                    .replace(std::path::MAIN_SEPARATOR, "/");
                require(files.insert(name), "duplicate staged soundd source path")?;
            }
        }
        Ok(())
    }
    let mut files = BTreeSet::new();
    visit(root, root, &mut files)?;
    Ok(files)
}

fn output_path(output: &Path) -> Result<PathBuf> {
    require(output.is_absolute(), "soundd output must be absolute")?;
    let name = output.file_name().ok_or("soundd output name missing")?;
    let parent = output.parent().ok_or("soundd output parent missing")?;
    require(
        fs::symlink_metadata(parent)?.file_type().is_dir(),
        "soundd output parent must be an existing real directory",
    )?;
    let parent = parent.canonicalize()?;
    let final_path = parent.join(name);
    match fs::symlink_metadata(&final_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(_) => return Err("soundd output already exists".into()),
        Err(error) => return Err(error.into()),
    }
    Ok(final_path)
}

fn require_retired_absent(root: &Path) -> Result<()> {
    match fs::symlink_metadata(root.join(RETIRED)) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err("retired soundd Python checker reappeared".into()),
        Err(error) => Err(error.into()),
    }
}

/// Copy the exact twenty retained source files into a fresh, atomically published root.
pub fn run(options: &StageOptions) -> Result<Value> {
    require(
        options.root.is_absolute(),
        "soundd source root must be absolute",
    )?;
    let root = options.root.canonicalize()?;
    require(
        fs::symlink_metadata(&root)?.file_type().is_dir(),
        "soundd source root must be a real directory",
    )?;
    let output = output_path(&options.output)?;
    let pins = source_pins()?;
    require_retired_absent(&root)?;
    let expected_live: BTreeMap<String, String> = pins
        .iter()
        .filter(|(name, _)| name.as_str() != RETIRED)
        .map(|(name, digest)| (name.clone(), digest.clone()))
        .collect();
    let mut held = Vec::with_capacity(COPIED_SOURCES);
    let mut identities = BTreeSet::new();
    let mut total = 0u64;
    for (name, digest) in &expected_live {
        let source = open_source(&root, name, digest)?;
        require(
            identities.insert((source.dev, source.ino)),
            format!("soundd source aliases another pinned file: {name}"),
        )?;
        total = total
            .checked_add(source.len)
            .ok_or("soundd total source size overflow")?;
        require(
            total <= MAX_TOTAL_BYTES,
            "soundd source set exceeds byte bound",
        )?;
        held.push(source);
    }

    let parent = output.parent().ok_or("soundd output parent missing")?;
    let staged = tempfile::Builder::new()
        .prefix(".soundd-source-")
        .tempdir_in(parent)?;
    for source in &mut held {
        let name = source.path.strip_prefix(&root)?;
        let digest = expected_live
            .get(&source.name)
            .ok_or("soundd staged source is not in pinned roster")?;
        copy_source(source, &staged.path().join(name), digest)?;
    }
    let roster = staged_roster(staged.path())?;
    let expected_roster: BTreeSet<String> = expected_live.keys().cloned().collect();
    require(
        roster == expected_roster && roster.len() == COPIED_SOURCES,
        "staged soundd source roster differs from retained manifest inputs",
    )?;
    for source in &held {
        verify_held(
            source,
            expected_live
                .get(&source.name)
                .ok_or("soundd held source is not pinned")?,
        )?;
    }
    require_retired_absent(&root)?;

    rustix::fs::renameat_with(
        rustix::fs::CWD,
        staged.path(),
        rustix::fs::CWD,
        &output,
        rustix::fs::RenameFlags::NOREPLACE,
    )?;
    let _published = staged.keep();

    let mut source_sha256 = Map::new();
    for (name, digest) in pins {
        source_sha256.insert(name, Value::String(digest));
    }
    Ok(json!({
        "status": "PASS",
        "source_sha256": source_sha256,
        "copied_files": COPIED_SOURCES,
        "staged_files": roster,
        "output": output,
        "source_bytes": total,
        "manifest_sha256": MANIFEST_SHA256,
        "limits": {
            "per_source_bytes": MAX_SOURCE_BYTES,
            "total_source_bytes": MAX_TOTAL_BYTES,
            "historical_checker_copied": false
        }
    }))
}

pub fn cli() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut root = None;
    let mut output = None;
    while let Some(option) = args.next() {
        let value = args
            .next()
            .ok_or("soundd source staging option requires a value")?;
        match option.to_str() {
            Some("--root") if root.is_none() => root = Some(PathBuf::from(value)),
            Some("--output") if output.is_none() => output = Some(PathBuf::from(value)),
            _ => return Err("expected unique --root and --output options".into()),
        }
    }
    let receipt = run(&StageOptions {
        root: root.ok_or("--root is required")?,
        output: output.ok_or("--output is required")?,
    })?;
    serde_json::to_writer(std::io::stdout().lock(), &receipt)?;
    writeln!(std::io::stdout().lock())?;
    Ok(())
}
