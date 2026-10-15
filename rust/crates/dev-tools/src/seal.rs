//! Seal complete native runtime inventories without a Python build dependency.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    error::Error,
    ffi::OsString,
    fs::{self, File},
    io::{Read, Write},
    os::unix::{ffi::OsStringExt, fs::PermissionsExt},
    path::{Component, Path, PathBuf},
};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;
pub const BOOT_BINARIES: [&str; 16] = [
    "openpilot-launch",
    "openpilot-runtime-install",
    "openpilot-manager",
    "openpilot-process-child",
    "openpilot-reboot",
    "openpilot-startup-recovery",
    "openpilot-carrot-recovery",
    "openpilot-carrot-web-watchdog",
    "openpilot-carrot-server",
    "openpilot-agnos",
    "openpilot-agnos-updater",
    "openpilot-text-window",
    "openpilot-usbgpu-model",
    "openpilot-usbgpu-worker",
    "openpilot-usbgpu-probe",
    "openpilot-bootlog",
];

fn argument_path(value: &Value) -> Result<PathBuf> {
    if let Some(path) = value.as_str() {
        return Ok(path.into());
    }
    if let Some(map) = value.as_object().filter(|m| m.len() == 1) {
        if let Some(bytes) = map.get("Unix").and_then(Value::as_array) {
            let bytes = bytes
                .iter()
                .map(|v| v.as_u64().and_then(|b| u8::try_from(b).ok()))
                .collect::<Option<Vec<_>>>();
            if let Some(bytes) = bytes {
                return Ok(OsString::from_vec(bytes).into());
            }
        }
    }
    Err("invalid manager argument encoding".into())
}
fn contained(root: &Path, path: &Path) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        root.join(path)
    };
    if !path.starts_with(root) || path.components().any(|c| c == Component::ParentDir) {
        return Err(format!("invalid artifact path: {}", path.display()).into());
    }
    // Required paths must exist; canonicalization also rejects dangling aliases.
    if !path.canonicalize()?.starts_with(root) {
        return Err(format!("artifact escapes repository: {}", path.display()).into());
    }
    Ok(path)
}
fn machine(architecture: &str) -> Result<u16> {
    match architecture {
        "aarch64" => Ok(183),
        "x86_64" => Ok(62),
        _ => Err("invalid architecture".into()),
    }
}
fn native(path: &Path, architecture: &str) -> Result<()> {
    let mut header = [0; 64];
    if File::open(path)?.read_exact(&mut header).is_err()
        || &header[..6] != b"\x7fELF\x02\x01"
        || u16::from_le_bytes([header[18], header[19]]) != machine(architecture)?
    {
        return Err(format!("expected {architecture} ELF64 artifact: {}", path.display()).into());
    }
    Ok(())
}
fn walk(root: &Path, directory: &Path, files: &mut BTreeSet<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = contained(root, &entry.path())?;
        let metadata = entry.metadata()?;
        if path.is_dir() {
            if metadata.file_type().is_symlink() {
                return Err(format!("unsupported directory symlink: {}", path.display()).into());
            }
            walk(root, &path, files)?;
        } else if path.is_file() {
            files.insert(path);
        } else {
            return Err(format!("unsupported artifact type: {}", path.display()).into());
        }
    }
    Ok(())
}
fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn digest(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut bytes = [0; 65536];
    loop {
        let count = file.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        hash.update(&bytes[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub fn seal(
    root: &Path,
    inventory: &Value,
    architecture: &str,
    commit: &str,
    required_boot: &[&str],
    required_companions: &[&str],
) -> Result<Value> {
    let root = root.canonicalize()?;
    machine(architecture)?;
    if !hex(commit, 40) {
        return Err("invalid source commit".into());
    }
    let entries = inventory
        .as_array()
        .filter(|v| !v.is_empty())
        .ok_or("empty or invalid manager inventory")?;
    let binaries = root.join("rust/bin");
    let mut executables = BTreeSet::new();
    let mut directories = BTreeSet::new();
    let mut files = BTreeSet::new();
    for name in required_boot {
        let path = binaries.join(name);
        if !path.is_file() {
            return Err(format!("missing native executable: {}", path.display()).into());
        }
        executables.insert(contained(&root, &path)?);
    }
    for name in required_companions {
        let path = binaries.join(name);
        if !path.is_dir() || fs::symlink_metadata(&path)?.file_type().is_symlink() {
            return Err(format!(
                "missing or unsupported bootstrap companion directory: {}",
                path.display()
            )
            .into());
        }
        directories.insert(contained(&root, &path)?);
    }
    for entry in entries {
        let argv = entry["argv"].as_array();
        if entry["ready"] != true || argv.is_none_or(Vec::is_empty) {
            return Err(format!(
                "catalog implementation unavailable: {}: {}",
                entry["name"], entry["error"]
            )
            .into());
        }
        let argv = argv.ok_or("missing manager argv")?;
        let path = argument_path(&argv[0])?;
        executables.insert(contained(&root, &path)?);
        let requirements = match entry.get("requirements") {
            None => &[][..],
            Some(value) => value
                .as_array()
                .ok_or("invalid manager requirements array")?
                .as_slice(),
        };
        for requirement in requirements {
            let fields = requirement
                .as_object()
                .ok_or("invalid requirement record")?;
            if fields
                .keys()
                .any(|key| !["path", "executable", "file", "directory"].contains(&key.as_str()))
            {
                return Err("unknown requirement field".into());
            }
            let mut kinds = 0;
            for flag in ["executable", "file", "directory"] {
                if let Some(value) = fields.get(flag) {
                    kinds += usize::from(value.as_bool().ok_or("invalid requirement flag")?);
                }
            }
            if kinds != 1 {
                return Err("requirement must have exactly one kind".into());
            }
            let path = Path::new(
                requirement["path"]
                    .as_str()
                    .ok_or("invalid requirement path")?,
            );
            let absolute = if path.is_absolute() {
                path.to_owned()
            } else {
                root.join(path)
            };
            if requirement["file"] == true
                && (!absolute.is_file() || fs::metadata(&absolute)?.len() == 0)
            {
                return Err(
                    format!("missing or empty required file: {}", absolute.display()).into(),
                );
            }
            let path = contained(&root, path)?;
            if requirement["executable"] == true {
                executables.insert(path);
            } else if requirement["file"] == true {
                let mut prefix = [0; 43];
                let count = File::open(&path)?.read(&mut prefix)?;
                if prefix[..count].starts_with(b"version https://git-lfs.github.com/spec/v1") {
                    return Err(format!(
                        "Git LFS pointer instead of required file: {}",
                        path.display()
                    )
                    .into());
                }
                files.insert(path);
            } else if requirement["directory"] == true {
                if !path.is_dir() {
                    return Err(format!(
                        "missing companion or source directory: {}",
                        path.display()
                    )
                    .into());
                }
                if path.starts_with(&binaries) {
                    if fs::symlink_metadata(&path)?.file_type().is_symlink() {
                        return Err(
                            format!("unsupported directory symlink: {}", path.display()).into()
                        );
                    }
                    directories.insert(path);
                }
            }
        }
    }
    for executable in &executables {
        if !executable.is_file() || fs::metadata(executable)?.permissions().mode() & 0o111 == 0 {
            return Err(format!("missing native executable: {}", executable.display()).into());
        }
        native(executable, architecture)?;
    }
    for directory in directories {
        let mut contents = BTreeSet::new();
        walk(&root, &directory, &mut contents)?;
        if contents.is_empty() {
            return Err(format!("empty companion directory: {}", directory.display()).into());
        }
    }
    walk(&root, &binaries, &mut files)?;
    if !executables.is_subset(&files) {
        return Err("catalog executable is outside rust/bin".into());
    }
    let mut rows = Vec::new();
    for path in files {
        let mut magic = [0; 4];
        let count = File::open(&path)?.read(&mut magic)?;
        if count == 4 && &magic == b"\x7fELF" {
            native(&path, architecture)?;
        }
        rows.push(json!({"path":path.strip_prefix(&root)?,"sha256":digest(&path)?,"executable":executables.contains(&path)}));
    }
    let manifest =
        json!({"schema":1,"source_commit":commit,"agnos_version":"19.8-carrot-bt1","files":rows});
    let destination = root.join("rust/runtime-manifest.json");
    let mut temporary =
        tempfile::NamedTempFile::new_in(destination.parent().ok_or("manifest parent missing")?)?;
    serde_json::to_writer_pretty(&mut temporary, &manifest)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary.persist(&destination).map_err(|e| e.error)?;
    File::open(destination.parent().ok_or("manifest parent missing")?)?.sync_all()?;
    Ok(manifest)
}
