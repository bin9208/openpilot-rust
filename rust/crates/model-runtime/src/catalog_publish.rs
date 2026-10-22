//! Finalize an already-emitted model catalog without loading model providers.
//!
//! This ports the generation/index/pointer tail of model_export.publish.publish.
//! The caller still owns tensor compilation, descriptors and its staging parent.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::CString,
    fs::{self, File, Metadata, OpenOptions},
    io::{Read, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const MAX_ENTRIES: usize = 16;
const MAX_FILES: usize = 20_000;
const MAX_TOTAL: u64 = 2 * 1024 * 1024 * 1024;
const MAX_REQUEST: u64 = 1024 * 1024;

// Field order is the existing Python json.dumps(sort_keys=True) index contract.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub camera: [u32; 2],
    pub directory: String,
    pub graph_sha256: String,
    pub kind: String,
    pub pipeline_sha256: String,
}
#[derive(Serialize)]
struct Index<'a> {
    bundles: &'a [Entry],
    version: u32,
}
#[derive(Debug, Serialize)]
pub struct Publication {
    pub generation: String,
    pub reused: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct Identity(u64, u64, u64, u32, i64, i64, i64, i64);
fn identity(m: &Metadata) -> Identity {
    Identity(
        m.dev(),
        m.ino(),
        m.len(),
        m.mode(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}
#[derive(Debug, PartialEq, Eq)]
enum Item {
    Directory,
    File(u64, String),
}
#[derive(Debug, PartialEq, Eq)]
struct Tree {
    items: BTreeMap<PathBuf, Item>,
    identities: BTreeMap<PathBuf, Identity>,
}
fn require(ok: bool, message: &'static str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn digest(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn open_regular(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)?;
    require(
        file.metadata()?.is_file(),
        "catalog input is not a regular file",
    )?;
    Ok(file)
}
fn bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let file = open_regular(path)?;
    let before = identity(&file.metadata()?);
    require(before.2 <= limit, "catalog input exceeds byte bound")?;
    let mut raw = Vec::new();
    (&file).take(limit + 1).read_to_end(&mut raw)?;
    require(
        raw.len() as u64 == before.2
            && identity(&file.metadata()?) == before
            && identity(&fs::symlink_metadata(path)?) == before,
        "catalog input changed while reading",
    )?;
    Ok(raw)
}
fn directory(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    require(file.metadata()?.is_dir(), "catalog path is not a directory")?;
    Ok(file)
}
fn same_directory(path: &Path, held: &File) -> Result<()> {
    let current = fs::symlink_metadata(path)?;
    let expected = held.metadata()?;
    require(
        current.is_dir() && current.dev() == expected.dev() && current.ino() == expected.ino(),
        "catalog directory replaced",
    )
}
fn tree(root: &Path) -> Result<Tree> {
    let held = directory(root)?;
    let mut found = Tree {
        items: BTreeMap::new(),
        identities: BTreeMap::new(),
    };
    let mut pending = vec![PathBuf::new()];
    let mut total = 0_u64;
    while let Some(relative) = pending.pop() {
        require(
            relative.components().count() <= 64,
            "catalog tree depth exceeds bound",
        )?;
        for entry in fs::read_dir(root.join(&relative))? {
            let entry = entry?;
            let name = relative.join(entry.file_name());
            let path = root.join(&name);
            let meta = fs::symlink_metadata(&path)?;
            require(
                !meta.file_type().is_symlink(),
                "catalog tree contains an alias",
            )?;
            let item = if meta.is_dir() {
                pending.push(name.clone());
                Item::Directory
            } else {
                require(meta.is_file(), "catalog tree contains a special file")?;
                total = total
                    .checked_add(meta.len())
                    .ok_or("catalog tree size overflow")?;
                require(total <= MAX_TOTAL, "catalog tree exceeds byte bound")?;
                let file = open_regular(&path)?;
                let before = identity(&meta);
                require(
                    identity(&file.metadata()?) == before,
                    "catalog file changed before hashing",
                )?;
                let mut hash = Sha256::new();
                let mut read = (&file).take(meta.len() + 1);
                let mut bytes = 0_u64;
                let mut chunk = [0_u8; 64 * 1024];
                loop {
                    let count = read.read(&mut chunk)?;
                    if count == 0 {
                        break;
                    }
                    hash.update(&chunk[..count]);
                    bytes += count as u64;
                }
                require(
                    bytes == meta.len()
                        && identity(&file.metadata()?) == before
                        && identity(&fs::symlink_metadata(&path)?) == before,
                    "catalog file changed while hashing",
                )?;
                Item::File(bytes, format!("{:x}", hash.finalize()))
            };
            found.identities.insert(name.clone(), identity(&meta));
            require(
                found.items.insert(name, item).is_none(),
                "duplicate catalog path",
            )?;
            require(
                found.items.len() <= MAX_FILES,
                "catalog tree file count exceeds bound",
            )?;
        }
    }
    same_directory(root, &held)?;
    Ok(found)
}
fn no_replace(from: &Path, to: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let from = CString::new(from.as_os_str().as_bytes())?;
        let to = CString::new(to.as_os_str().as_bytes())?;
        // SAFETY: both paths are live NUL-terminated byte strings. The syscall
        // retains neither pointer; NOREPLACE protects a concurrently-created path.
        let status = unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                from.as_ptr(),
                libc::AT_FDCWD,
                to.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if status != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (from, to);
        Err("native catalog finalization currently requires Linux renameat2".into())
    }
}
/// Finalize a caller-owned staging directory at ROOT/.build-*/generation.
/// The request is a bounded JSON array of the emitter's actual BundleEntry data.
/// This does not certify compiled kernels, ABI, numerical behavior or retirement.
pub fn publish(root: &Path, staged: &Path, request: &Path) -> Result<Publication> {
    let raw = bounded(request, MAX_REQUEST)?;
    let entries: Vec<Entry> = serde_json::from_slice(&raw)?;
    require(
        !entries.is_empty() && entries.len() <= MAX_ENTRIES,
        "empty or excessive pipeline bundle selection",
    )?;
    let mut keys = BTreeSet::new();
    for entry in &entries {
        require(
            matches!(entry.kind.as_str(), "driving" | "driver")
                && matches!(entry.camera, [1928, 1208] | [1344, 760]),
            "unsupported pipeline kind or camera",
        )?;
        require(
            keys.insert((&entry.kind, entry.camera)),
            "duplicate pipeline bundle selection",
        )?;
        require(
            entry.directory == format!("{}-{}x{}", entry.kind, entry.camera[0], entry.camera[1]),
            "pipeline bundle directory differs from identity",
        )?;
        require(
            is_digest(&entry.graph_sha256) && is_digest(&entry.pipeline_sha256),
            "pipeline SHA-256 provenance required",
        )?;
    }
    let root_held = directory(root)?;
    let root = fs::canonicalize(root)?;
    let staged_held = directory(staged)?;
    let staged = fs::canonicalize(staged)?;
    let parent = staged.parent().ok_or("catalog stage parent missing")?;
    require(
        parent.parent() == Some(root.as_path())
            && parent
                .file_name()
                .is_some_and(|v| v.as_bytes().starts_with(b".build-"))
            && staged.file_name().is_some_and(|v| v == "generation"),
        "catalog stage must be ROOT/.build-*/generation",
    )?;
    let parent_held = directory(parent)?;
    let allowed: BTreeSet<_> = entries
        .iter()
        .map(|entry| PathBuf::from(&entry.directory))
        .collect();
    for child in fs::read_dir(&staged)? {
        let child = child?;
        require(
            allowed.contains(&PathBuf::from(child.file_name())),
            "unexpected staged catalog member",
        )?;
    }
    for entry in &entries {
        let bundle = staged.join(&entry.directory);
        directory(&bundle)?;
        let graph = bounded(&bundle.join("graph.json"), 64 * 1024 * 1024)?;
        let pipeline = bounded(&bundle.join("pipeline.json"), MAX_REQUEST)?;
        require(
            digest(&graph) == entry.graph_sha256 && digest(&pipeline) == entry.pipeline_sha256,
            "emitted pipeline hashes disagree with entries",
        )?;
    }
    let mut index = serde_json::to_vec_pretty(&Index {
        bundles: &entries,
        version: 1,
    })?;
    index.push(b'\n');
    let generation = digest(&index);
    openpilot_model_runtime::compiler_backend_cpu::llvm::require_output_space(&root)?;
    // Existing input/index files are never overwritten. Outputs removes only an
    // index inode created by this invocation if a prepublication check fails.
    let mut index_output = openpilot_model_runtime::owned_outputs::Outputs::default();
    index_output.write(&staged.join("index.json"), &index)?;
    let expected = tree(&staged)?;
    same_directory(&staged, &staged_held)?;
    same_directory(parent, &parent_held)?;
    same_directory(&root, &root_held)?;
    let destination = root.join(&generation);
    let reused = match fs::symlink_metadata(&destination) {
        Ok(_) => {
            let existing = tree(&destination)?;
            require(
                existing.items == expected.items,
                "existing generation disagrees with immutable artifact",
            )?;
            require(
                tree(&destination)? == existing,
                "existing generation changed during verification",
            )?;
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    require(
        tree(&staged)? == expected,
        "staged generation changed during publication",
    )?;
    // Commit ownership before moving the directory: the original Python tail
    // also retains an immutable generation if pointer publication later fails.
    index_output.commit()?;
    if !reused {
        no_replace(&staged, &destination)?;
    }
    same_directory(&root, &root_held)?;
    same_directory(parent, &parent_held)?;
    let destination_held = directory(&destination)?;
    require(
        tree(&destination)?.items == expected.items,
        "published generation changed before pointer update",
    )?;
    same_directory(&destination, &destination_held)?;
    let mut pointer = tempfile::Builder::new()
        .prefix(".current-")
        .tempfile_in(parent)?;
    writeln!(
        pointer,
        "{{\"generation\": \"{generation}\", \"version\": 1}}"
    )?;
    pointer.as_file().sync_all()?;
    pointer.persist(root.join("current.json"))?;
    Ok(Publication { generation, reused })
}
