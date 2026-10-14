//! Stage source-locked NumPy native numerical providers without importing Python.
use crate::{disk_space, seal::Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{Cursor, Read},
    path::{Path, PathBuf},
};
const VERSION: &str = "2.5.3";
const MAX_WHEEL: u64 = 128 * 1024 * 1024;
const MAX_EXPANDED: u64 = 512 * 1024 * 1024;
fn require(value: bool, reason: &str) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(reason.into())
    }
}
fn bounded(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(maximum + 1)
        .read_to_end(&mut bytes)?;
    require(
        bytes.len() as u64 <= maximum,
        "numerical staging input exceeds limit",
    )?;
    Ok(bytes)
}
fn safe(name: &str) -> Result<()> {
    require(
        !name.is_empty() && !name.starts_with('/') && !name.contains(['\\', '\0']),
        "unsafe numerical wheel member",
    )?;
    require(
        name.trim_end_matches('/')
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != ".."),
        "unsafe numerical wheel member",
    )
}
#[derive(Serialize)]
struct Library {
    name: String,
    sha256: String,
}
#[derive(Serialize)]
struct Manifest {
    format: u8,
    numpy: &'static str,
    abi: &'static str,
    library: String,
    files: Vec<Library>,
}
#[derive(Serialize)]
pub struct Provenance {
    wheel: String,
    sha256: String,
    source: &'static str,
    runtime: &'static str,
}
fn json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}
pub fn stage(wheel: &Path, output: &Path, lock: &Path) -> Result<Provenance> {
    require(
        !output.exists() && !output.is_symlink(),
        "numerical output already exists",
    )?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    disk_space::inspect(parent, MAX_WHEEL + MAX_EXPANDED + 1024 * 1024)?.require()?;
    let filename = wheel
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("invalid numerical wheel filename")?;
    let lock: toml::Value = std::str::from_utf8(&bounded(lock, 4 * 1024 * 1024)?)?.parse()?;
    let packages = lock
        .get("package")
        .and_then(|v| v.as_array())
        .ok_or("missing locked packages")?;
    let packages: Vec<_> = packages
        .iter()
        .filter(|p| p.get("name").and_then(|v| v.as_str()) == Some("numpy"))
        .collect();
    require(
        packages.len() == 1,
        "expected exactly one source-locked NumPy package",
    )?;
    let package = packages[0];
    require(
        package.get("version").and_then(|v| v.as_str()) == Some(VERSION),
        "torqued numerical ABI must be reviewed when NumPy lock changes",
    )?;
    let wheels = package
        .get("wheels")
        .and_then(|v| v.as_array())
        .ok_or("missing locked numerical wheels")?;
    let pins: Vec<_> = wheels
        .iter()
        .filter(|p| {
            p.get("url")
                .and_then(|v| v.as_str())
                .and_then(|url| url.rsplit('/').next())
                == Some(filename)
        })
        .collect();
    require(
        pins.len() == 1,
        "numerical wheel must match an exact NumPy2.5.3 archive in uv.lock",
    )?;
    let bytes = bounded(wheel, MAX_WHEEL)?;
    let identity = format!("{:x}", Sha256::digest(&bytes));
    require(
        pins[0].get("hash").and_then(|v| v.as_str()) == Some(format!("sha256:{identity}").as_str()),
        "numerical wheel checksum differs from uv.lock",
    )?;
    if let Some(size) = pins[0].get("size") {
        require(
            size.as_integer().and_then(|n| u64::try_from(n).ok()) == Some(bytes.len() as u64),
            "numerical wheel size differs from uv.lock",
        )?;
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    require(archive.len() <= 100000, "numerical wheel member limit")?;
    let mut selected = Vec::new();
    let mut outputs = BTreeSet::new();
    let mut expanded = 0u64;
    let mut names = BTreeSet::new();
    for index in 0..archive.len() {
        let member = archive.by_index(index)?;
        let name = member.name()?;
        safe(&name)?;
        require(
            names.insert(name.to_string()),
            "duplicate numerical wheel member",
        )?;
        let kind = member.unix_mode().unwrap_or(0) & 0o170000;
        require(
            matches!(kind, 0 | 0o100000 | 0o040000),
            "numerical wheel link or special member",
        )?;
        if member.is_dir() && !name.starts_with("numpy.libs/") {
            continue;
        }
        let target = if name.starts_with("numpy.libs/") {
            Some(PathBuf::from(
                name.trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .ok_or("missing native filename")?,
            ))
        } else if name.contains(".dist-info/licenses/") {
            Some(
                Path::new("licenses").join(
                    name.split_once("/licenses/")
                        .ok_or("invalid numerical license path")?
                        .1,
                ),
            )
        } else {
            None
        };
        if let Some(target) = target {
            require(
                member.size() <= MAX_WHEEL,
                "numerical library or license exceeds limit",
            )?;
            expanded = expanded
                .checked_add(member.size())
                .ok_or("numerical size overflow")?;
            require(
                expanded <= MAX_EXPANDED,
                "numerical wheel expansion exceeds limit",
            )?;
            require(
                outputs.insert(target.clone()),
                "numerical wheel output collision",
            )?;
            selected.push((
                index,
                target,
                member.size(),
                name.starts_with("numpy.libs/"),
            ));
        }
    }
    let libraries: Vec<_> = selected
        .iter()
        .filter(|(_, p, _, library)| {
            *library
                && p.to_str()
                    .is_some_and(|p| p.starts_with("libscipy_openblas64_"))
        })
        .collect();
    require(
        libraries.len() == 1,
        "expected exactly one native ILP64 OpenBLAS library",
    )?;
    let library = libraries[0]
        .1
        .to_str()
        .ok_or("invalid OpenBLAS filename")?
        .to_owned();
    require(
        selected.iter().any(|(_, _, _, native)| !native),
        "numerical wheel has no provider licenses",
    )?;
    disk_space::inspect(parent, expanded + MAX_WHEEL + 1024 * 1024)?.require()?;
    fs::create_dir_all(parent)?;
    let temporary = tempfile::tempdir_in(parent)?;
    let mut files = Vec::new();
    for (index, target, size, native) in selected {
        let mut member = archive.by_index(index)?;
        let mut data = Vec::new();
        member.by_ref().take(size + 1).read_to_end(&mut data)?;
        require(
            data.len() as u64 == size,
            "numerical wheel member size changed",
        )?;
        let target = temporary.path().join(target);
        fs::create_dir_all(target.parent().ok_or("missing numerical output parent")?)?;
        fs::write(&target, &data)?;
        if native {
            files.push(Library {
                name: target
                    .file_name()
                    .and_then(|v| v.to_str())
                    .ok_or("invalid native library name")?
                    .to_owned(),
                sha256: format!("{:x}", Sha256::digest(&data)),
            });
        }
    }
    json(
        &temporary.path().join("manifest.json"),
        &Manifest {
            format: 1,
            numpy: VERSION,
            abi: "scipy_dgesdd_64_",
            library,
            files,
        },
    )?;
    let provenance = Provenance {
        wheel: filename.to_owned(),
        sha256: identity,
        source: "https://github.com/numpy/numpy/tree/v2.5.3",
        runtime: "native shared libraries only; no Python imports",
    };
    json(&temporary.path().join("provenance.json"), &provenance)?;
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        temporary.path(),
        rustix::fs::CWD,
        output,
        rustix::fs::RenameFlags::NOREPLACE,
    )?;
    Ok(provenance)
}
