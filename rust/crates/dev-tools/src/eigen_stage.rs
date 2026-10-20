//! Stage only the Eigen 3.4.0 header tree from a pinned commaai wheel.
use crate::{disk_space, seal::Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{self, Cursor, Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

const VERSION: &str = "3.4.0";
const SOURCE_COMMIT: &str = "40e5d76de1b33a86c5181b63db6782d8f06da1da";
const RELEASE: &str = "https://github.com/commaai/dependencies/releases/download/eigen/v3.4.0";
const PREFIX: &str = "eigen-3.4.0.data/purelib/eigen/install/";
const WHEEL_LIMIT: u64 = 3 * 1024 * 1024;
const EXPANDED_LIMIT: u64 = 16 * 1024 * 1024;
const FILE_COUNT: usize = 690;
const EXPANDED_BYTES: u64 = 10_130_800;

struct Pin {
    architecture: &'static str,
    filename: &'static str,
    sha256: &'static str,
    bytes: u64,
}
const PINS: [Pin; 3] = [
    Pin {
        architecture: "x86_64",
        filename: "eigen-3.4.0-py3-none-linux_x86_64.whl",
        sha256: "0680581b726b6160646d9053cc7416f68c9a38353a26a7f4e4a58222233aa1ba",
        bytes: 2_250_658,
    },
    Pin {
        architecture: "aarch64",
        filename: "eigen-3.4.0-py3-none-linux_aarch64.whl",
        sha256: "b02ee704aa964d9d7bafb53cc0e096680e4a3b4f6fb1cd98b0a85c7298f3dc0a",
        bytes: 2_250_657,
    },
    Pin {
        architecture: "macos-arm64",
        filename: "eigen-3.4.0-py3-none-macosx_11_0_arm64.whl",
        sha256: "a733325c7290984f270a0ea1f4af0c6aca4c9cf1dfedec262767098cf8e4f355",
        bytes: 2_250_659,
    },
];

fn require(ok: bool, reason: &'static str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(reason.into())
    }
}
fn safe(name: &str) -> Result<()> {
    require(
        !name.is_empty()
            && !name.starts_with('/')
            && !name.contains(['\\', '\0'])
            && name.len() <= 4096
            && name
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != ".."),
        "unsafe Eigen wheel member",
    )
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn read_wheel(path: &Path) -> Result<Vec<u8>> {
    let nonblock =
        i32::try_from((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits())?;
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(nonblock)
        .open(path)?;
    require(
        file.metadata()?.is_file(),
        "Eigen wheel is not a regular file",
    )?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(WHEEL_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    require(
        bytes.len() as u64 <= WHEEL_LIMIT,
        "Eigen wheel exceeds limit",
    )?;
    Ok(bytes)
}
#[derive(Serialize)]
pub struct FileRecord {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Serialize)]
pub struct Manifest {
    pub format: u8,
    pub version: &'static str,
    pub source_commit: &'static str,
    pub release: &'static str,
    pub architecture: &'static str,
    pub wheel_url: String,
    pub wheel_sha256: &'static str,
    pub wheel_bytes: u64,
    pub files: Vec<FileRecord>,
}
struct Member {
    index: usize,
    path: PathBuf,
    bytes: u64,
}
fn central_directory_count(bytes: &[u8], start: u64) -> Result<usize> {
    let mut offset = usize::try_from(start)?;
    let mut count = 0_usize;
    loop {
        let signature = bytes
            .get(offset..offset.checked_add(4).ok_or("Eigen ZIP offset overflow")?)
            .ok_or("truncated Eigen ZIP directory")?;
        if signature != b"PK\x01\x02" {
            require(
                signature == b"PK\x05\x06" || signature == b"PK\x06\x06",
                "invalid Eigen ZIP directory terminator",
            )?;
            return Ok(count);
        }
        let header = bytes
            .get(offset..offset.checked_add(46).ok_or("Eigen ZIP offset overflow")?)
            .ok_or("truncated Eigen ZIP directory record")?;
        let trailing = [28, 30, 32]
            .into_iter()
            .map(|index| usize::from(u16::from_le_bytes([header[index], header[index + 1]])))
            .sum::<usize>();
        offset = offset
            .checked_add(46)
            .and_then(|value| value.checked_add(trailing))
            .ok_or("Eigen ZIP directory overflow")?;
        require(offset <= bytes.len(), "Eigen ZIP directory outside wheel")?;
        count += 1;
        require(count <= 1_000, "Eigen wheel member limit")?;
    }
}
fn inspect_archive(bytes: &[u8]) -> Result<Vec<Member>> {
    let mut wheel = zip::ZipArchive::new(Cursor::new(bytes))?;
    require(wheel.len() <= 1_000, "Eigen wheel member limit")?;
    require(
        central_directory_count(bytes, wheel.central_directory_start())? == wheel.len(),
        "duplicate Eigen wheel central-directory member",
    )?;
    let mut names = BTreeSet::<String>::new();
    let mut selected = Vec::new();
    let mut expanded = 0_u64;
    for index in 0..wheel.len() {
        let member = wheel.by_index(index)?;
        let name = member.name()?.into_owned();
        safe(&name)?;
        require(
            names.insert(name.to_owned()),
            "duplicate Eigen wheel member",
        )?;
        let mode = member.unix_mode().unwrap_or(0) & 0o170000;
        require(
            mode == 0 || mode == 0o100000,
            "Eigen wheel link or special member",
        )?;
        require(!member.is_dir(), "unexpected Eigen wheel directory member")?;
        if let Some(relative) = name.strip_prefix(PREFIX) {
            safe(relative)?;
            require(
                !relative.ends_with(".py"),
                "Python source in Eigen install tree",
            )?;
            expanded = expanded
                .checked_add(member.size())
                .ok_or("Eigen wheel expanded size overflow")?;
            require(
                expanded <= EXPANDED_LIMIT,
                "Eigen wheel expansion exceeds limit",
            )?;
            selected.push(Member {
                index,
                path: PathBuf::from(relative),
                bytes: member.size(),
            });
        }
    }
    require(
        selected.len() == FILE_COUNT,
        "incomplete Eigen install tree",
    )?;
    require(expanded == EXPANDED_BYTES, "Eigen install size changed")?;
    Ok(selected)
}
/// Create a new output directory containing `install/` and a complete hash manifest.
/// A wrong wheel, unsafe member or existing destination is a hard failure.
pub fn stage(wheel: &Path, output: &Path, architecture: &str) -> Result<Manifest> {
    let pin = PINS
        .iter()
        .find(|pin| pin.architecture == architecture)
        .ok_or("unsupported Eigen wheel architecture")?;
    require(
        wheel.file_name().and_then(|name| name.to_str()) == Some(pin.filename),
        "Eigen wheel filename differs from release pin",
    )?;
    require(
        fs::symlink_metadata(output).is_err_and(|error| error.kind() == io::ErrorKind::NotFound),
        "Eigen output already exists",
    )?;
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    disk_space::inspect(parent, WHEEL_LIMIT + EXPANDED_LIMIT + 1024 * 1024)?.require()?;
    let bytes = read_wheel(wheel)?;
    require(
        bytes.len() as u64 == pin.bytes,
        "Eigen wheel size differs from release pin",
    )?;
    require(
        hash(&bytes) == pin.sha256,
        "Eigen wheel SHA-256 differs from release pin",
    )?;
    let selected = inspect_archive(&bytes)?;
    fs::create_dir_all(parent)?;
    let temporary = tempfile::Builder::new()
        .prefix(".eigen-stage-")
        .tempdir_in(parent)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut files = Vec::with_capacity(selected.len());
    for member in selected {
        let target = temporary.path().join("install").join(&member.path);
        fs::create_dir_all(target.parent().ok_or("Eigen output parent missing")?)?;
        let mut entry = archive.by_index(member.index)?;
        let mut file = File::create(&target)?;
        let mut digest = Sha256::new();
        let mut copied = 0_u64;
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let count = entry.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            copied = copied
                .checked_add(count as u64)
                .ok_or("Eigen file size overflow")?;
            require(copied <= member.bytes, "Eigen member exceeds ZIP size")?;
            file.write_all(&buffer[..count])?;
            digest.update(&buffer[..count]);
        }
        require(
            copied == member.bytes,
            "Eigen member size differs from ZIP record",
        )?;
        files.push(FileRecord {
            path: member
                .path
                .to_str()
                .ok_or("non-UTF8 Eigen path")?
                .to_owned(),
            bytes: copied,
            sha256: format!("{:x}", digest.finalize()),
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    let manifest = Manifest {
        format: 1,
        version: VERSION,
        source_commit: SOURCE_COMMIT,
        release: RELEASE,
        architecture: pin.architecture,
        wheel_url: format!("{RELEASE}/{}", pin.filename),
        wheel_sha256: pin.sha256,
        wheel_bytes: pin.bytes,
        files,
    };
    let mut file = File::create(temporary.path().join("manifest.json"))?;
    serde_json::to_writer_pretty(&mut file, &manifest)?;
    file.write_all(b"\n")?;
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        temporary.path(),
        rustix::fs::CWD,
        output,
        rustix::fs::RenameFlags::NOREPLACE,
    )?;
    let _published = temporary.keep();
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wheel(path: &Path, names: &[&str]) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        for name in names {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"fixture").unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn malformed_paths_and_python_inside_install_fail_before_publication() {
        let temp = tempfile::tempdir().unwrap();
        for (index, name, reason) in [
            (
                0,
                "eigen-3.4.0.data/purelib/eigen/install/../escape",
                "unsafe",
            ),
            (
                1,
                "eigen-3.4.0.data/purelib/eigen/install/eigen3/Eigen/\\escape",
                "unsafe",
            ),
            (
                2,
                "eigen-3.4.0.data/purelib/eigen/install/import.py",
                "Python source",
            ),
        ] {
            let path = temp.path().join(format!("{index}.whl"));
            wheel(&path, &[name]);
            let bytes = fs::read(&path).unwrap();
            assert!(inspect_archive(&bytes)
                .err()
                .unwrap()
                .to_string()
                .contains(reason));
        }
    }

    #[test]
    fn duplicate_and_missing_install_members_fail() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("duplicate.whl");
        let member = "eigen-3.4.0.data/purelib/eigen/install/eigen3/Eigen/Core";
        let alias = "eigen-3.4.0.data/purelib/eigen/install/eigen3/Eigen/Cora";
        wheel(&path, &[member, alias]);
        let mut duplicate = fs::read(&path).unwrap();
        let central_name = duplicate
            .windows(alias.len())
            .rposition(|bytes| bytes == alias.as_bytes())
            .unwrap();
        duplicate[central_name..central_name + alias.len()].copy_from_slice(member.as_bytes());
        assert!(inspect_archive(&duplicate)
            .err()
            .unwrap()
            .to_string()
            .contains("duplicate"));

        wheel(&path, &[member]);
        assert!(inspect_archive(&fs::read(&path).unwrap())
            .err()
            .unwrap()
            .to_string()
            .contains("incomplete"));
        assert!(central_directory_count(b"PK\x01\x02", 0).is_err());
    }
}
