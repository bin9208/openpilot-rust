//! Authenticate a relocatable ARM Python installation without running installers.
//! Expected contents come from the pinned vendor archive, including its notices.
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

const UNPACKED_LIMIT: u64 = 256 * 1024 * 1024;
const PROVENANCE: &str = "provenance/native-python.json";
const REQUIRED: &[&str] = &[
    "bin/python3.12",
    "include/python3.12/Python.h",
    "lib/python3.12/os.py",
    "lib/python3.12/encodings/__init__.py",
];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema: u32,
    pub version: String,
    pub target: String,
    pub release: String,
    pub reserve_bytes: u64,
    pub asset: assets::Asset,
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
    pub version: String,
    pub target: String,
    pub root: PathBuf,
    pub python: PathBuf,
    pub tree_receipt: PathBuf,
    pub tree_sha256: String,
    pub asset: assets::Receipt,
    pub copied_bytes: u64,
    pub expected_additional_bytes: u64,
    pub elapsed_ms: u128,
    pub installer_executed: bool,
    pub interpreter_executed: bool,
    pub runtime_activated: bool,
}

impl Plan {
    pub fn read(path: &Path) -> Result<Self> {
        let input = regular_file(path)?;
        if input.metadata()?.len() > 65536 {
            return Err("Python toolchain plan exceeds 64 KiB".into());
        }
        let plan: Self = serde_json::from_reader(input.take(65537))?;
        plan.validate()?;
        Ok(plan)
    }
    fn provenance(&self) -> Result<Vec<u8>> {
        // Admission estimates are not Python inputs. Revising an estimate must
        // not copy the same interpreter into another content generation.
        json(&(
            "openpilot-native-python-v1",
            &self.version,
            &self.target,
            &self.release,
            &self.asset,
        ))
    }
    fn assets(&self) -> assets::Plan {
        assets::Plan {
            schema: 1,
            reserve_bytes: 0,
            assets: BTreeMap::from([("native-python".into(), self.asset.clone())]),
        }
    }
    fn validate(&self) -> Result<()> {
        if self.schema != 1
            || self.version != "3.12.16"
            || self.target != "aarch64-unknown-linux-gnu"
            || self.release != "20261026"
            || self.asset.bytes > 64 * 1024 * 1024
            || self.reserve_bytes < 1024 * 1024
            || self.reserve_bytes > 64 * 1024 * 1024
            || self.asset.url != format!(
                "https://github.com/astral-sh/python-build-standalone/releases/download/{}/cpython-{}%2B{}-{}-install_only_stripped.tar.gz",
                self.release, self.version, self.release, self.target
            )
        {
            return Err("unsupported pinned native Python distribution".into());
        }
        self.assets().validate()
    }
}

fn unpack(plan: &Plan, archive: &Path, destination: Option<&Path>) -> Result<Tree> {
    let decoder = flate2::read::MultiGzDecoder::new(regular_file(archive)?);
    let mut tar = tar::Archive::new(decoder.take(UNPACKED_LIMIT + 1));
    let mut entries = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut total = 0_u64;
    let mut notices = 0;
    for member in tar.entries()? {
        let mut member = member?;
        let name = member.path()?.into_owned();
        if !normal(&name)
            || name.as_os_str().len() > 4096
            || !seen.insert(name.clone())
            || seen.len() > 50_000
        {
            return Err("unsafe, duplicate or excessive Python archive member".into());
        }
        let path = name.strip_prefix("python")?;
        if path.as_os_str().is_empty() {
            if !member.header().entry_type().is_dir() {
                return Err("Python archive root is not a directory".into());
            }
            continue;
        }
        if !normal(path) || path.starts_with("provenance") {
            return Err("Python archive member overlaps generated provenance".into());
        }
        let size = member.size();
        total = total
            .checked_add(size)
            .ok_or("Python unpacked size overflow")?;
        if total > UNPACKED_LIMIT || size > 128 * 1024 * 1024 {
            return Err("Python archive exceeds its bounded expansion estimate".into());
        }
        let mode = member.header().mode()?;
        if mode & 0o7000 != 0 {
            return Err("Python archive has special file permission bits".into());
        }
        let entry = match member.header().entry_type() {
            kind if kind.is_dir() && size == 0 => {
                if let Some(root) = destination {
                    parents(root, path)?;
                    directory(&root.join(path))?;
                }
                Entry::Directory
            }
            kind if kind.is_symlink() && size == 0 => {
                let target = member
                    .link_name()?
                    .ok_or("Python symlink lacks a target")?
                    .into_owned();
                if target.is_absolute()
                    || target.as_os_str().is_empty()
                    || target.as_os_str().len() > 4096
                {
                    return Err("Python archive link is not contained".into());
                }
                Entry::Symlink { target }
            }
            kind if kind.is_file() => {
                let mode = if mode & 0o111 == 0 { 0o444 } else { 0o555 };
                let mut output = if let Some(root) = destination {
                    parents(root, path)?;
                    Some(
                        OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
                            .open(root.join(path))?,
                    )
                } else {
                    None
                };
                let mut digest = Sha256::new();
                let mut count = 0_u64;
                let mut buffer = [0; 65536];
                loop {
                    let n = member.read(&mut buffer)?;
                    if n == 0 {
                        break;
                    }
                    count += n as u64;
                    digest.update(&buffer[..n]);
                    if let Some(file) = &mut output {
                        file.write_all(&buffer[..n])?;
                    }
                }
                if count != size {
                    return Err("Python archive member changed size".into());
                }
                if let Some(file) = output {
                    file.set_permissions(fs::Permissions::from_mode(mode))?;
                    file.sync_all()?;
                }
                if path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        let name = name.to_ascii_lowercase();
                        name.contains("license") || name.contains("copying")
                    })
                    && size > 0
                {
                    notices += 1;
                }
                Entry::File {
                    sha256: format!("{:x}", digest.finalize()),
                    bytes: size,
                    mode,
                }
            }
            _ => return Err("unsupported Python archive member type".into()),
        };
        insert(&mut entries, path.to_owned(), entry)?;
    }
    let mut decoder = tar.into_inner();
    let mut padding = 0;
    let mut buffer = [0; 10240];
    loop {
        let n = decoder.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        padding += n;
        if padding > 10240 || buffer[..n].iter().any(|byte| *byte != 0) {
            return Err("unexpected Python archive tail".into());
        }
    }
    if decoder.limit() == 0
        || notices == 0
        || REQUIRED
            .iter()
            .any(|path| !matches!(entries.get(Path::new(path)), Some(Entry::File { .. })))
    {
        return Err("Python archive is incomplete or exceeds its expansion bound".into());
    }
    if !matches!(
        entries.get(Path::new("bin/python3.12")),
        Some(Entry::File { mode: 0o555, .. })
    ) {
        return Err("Python interpreter is not an executable regular archive member".into());
    }
    let provenance = plan.provenance()?;
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
    links(&tree, destination)?;
    if let Some(root) = destination {
        file(root, Path::new(PROVENANCE), &provenance, 0o444)?;
        tree.verify(root)?;
        openpilot_runtime_installer::verify_native_elf(&root.join("bin/python3.12"), true)?;
    }
    Ok(tree)
}

pub fn prepare(
    plan: &Plan,
    cache: &Path,
    store: &Path,
    download: &impl assets::Download,
) -> Result<Prepared> {
    let started = Instant::now();
    plan.validate()?;
    directory(store)?;
    let store = store.canonicalize()?;
    let identity = hash(&plan.provenance()?);
    let object = store.join(&identity);
    directory(&object)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(object.join("lock"))?;
    if !lock.metadata()?.is_file() {
        return Err("Python input lock is not regular".into());
    }
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive)?;
    let generation = object.join("generation");
    let existing = match fs::symlink_metadata(&generation) {
        Ok(metadata) if metadata.is_dir() => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Ok(_) => return Err("Python input generation is not a real directory".into()),
        Err(error) => return Err(error.into()),
    };
    // The authenticated archive remains the independent expected-tree source on
    // reuse. A published directory/receipt never establishes its own contents.
    let mut assets = plan.assets();
    assets.reserve_bytes = if existing {
        0
    } else {
        UNPACKED_LIMIT + plan.reserve_bytes
    };
    if !existing {
        let space = rustix::fs::statvfs(&object)?;
        if space
            .f_bavail
            .checked_mul(space.f_frsize)
            .ok_or("Python space overflow")?
            < UNPACKED_LIMIT + plan.reserve_bytes
        {
            return Err(
                "insufficient capacity for this Python prefix; previous inputs retained".into(),
            );
        }
    }
    let asset = assets::prepare(cache, &assets, download)?
        .remove("native-python")
        .ok_or("missing Python archive")?;
    let tree = unpack(plan, &asset.path, None)?;
    let bytes = json(&tree)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("Python tree receipt exceeds the consumer's 16 MiB limit".into());
    }
    let receipt = Installation {
        schema: 1,
        identity: identity.clone(),
        tree_sha256: hash(&bytes),
    };
    let root = generation.join("tree");
    let copied_bytes = if existing {
        let input = regular_file(&generation.join("receipt.json"))?;
        if input.metadata()?.len() > 65536
            || regular_file(&generation.join("tree.json"))?
                .metadata()?
                .len()
                != bytes.len() as u64
        {
            return Err("Python installation metadata exceeds expected bounds".into());
        }
        let actual: Installation = serde_json::from_reader(input.take(65537))?;
        if actual != receipt || digest_file(&generation.join("tree.json"))? != receipt.tree_sha256 {
            return Err(
                "Python installation receipt differs from the authenticated archive".into(),
            );
        }
        tree.verify(&root)?;
        openpilot_runtime_installer::verify_native_elf(&root.join("bin/python3.12"), true)?;
        0
    } else {
        let temporary = tempfile::Builder::new()
            .prefix(".pending-")
            .tempdir_in(&object)?;
        let staged = temporary.path().join("tree");
        directory(&staged)?;
        let actual = unpack(plan, &asset.path, Some(&staged))?;
        if actual != tree {
            return Err("Python archive changed during preparation".into());
        }
        file(temporary.path(), Path::new("tree.json"), &bytes, 0o444)?;
        file(
            temporary.path(),
            Path::new("receipt.json"),
            &json(&receipt)?,
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
        tree.entries
            .values()
            .map(|entry| match entry {
                Entry::File { bytes, .. } => *bytes,
                _ => 0,
            })
            .sum()
    };
    Ok(Prepared {
        schema: 1,
        identity,
        version: plan.version.clone(),
        target: plan.target.clone(),
        python: root.join("bin/python3.12"),
        root,
        tree_receipt: generation.join("tree.json"),
        tree_sha256: receipt.tree_sha256,
        expected_additional_bytes: asset.stored_bytes
            + if existing {
                0
            } else {
                UNPACKED_LIMIT + plan.reserve_bytes
            },
        asset,
        copied_bytes,
        elapsed_ms: started.elapsed().as_millis(),
        installer_executed: false,
        interpreter_executed: false,
        runtime_activated: false,
    })
}
