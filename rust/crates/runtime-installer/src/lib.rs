mod activation;
mod archive;
mod identity;
pub mod transaction;
mod transport;

use identity::{Descriptor, Manifest, MAX_JSON_BYTES};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    process::Command,
};
pub use transport::{CurlDownload, Download};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct Options {
    pub root: PathBuf,
    pub check: bool,
    pub offline: Option<(PathBuf, PathBuf)>,
    pub reserve_bytes: u64,
}
const DEFAULT_RESERVE: u64 = 2 * 1024 * 1024 * 1024;
pub(crate) const CACHED_ARCHIVE: &str = ".archive.tar.gz";

impl Options {
    pub fn parse(args: impl Iterator<Item = std::ffi::OsString>) -> Result<Self> {
        let mut args = args;
        let mut root = None;
        let mut archive = None;
        let mut descriptor = None;
        let mut check = false;
        let mut reserve_bytes = DEFAULT_RESERVE;
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--root") if root.is_none() => root = Some(PathBuf::from(value(&mut args)?)),
                Some("--archive") if archive.is_none() => archive = Some(PathBuf::from(value(&mut args)?)),
                Some("--descriptor") if descriptor.is_none() => descriptor = Some(PathBuf::from(value(&mut args)?)),
                Some("--reserve-bytes") => reserve_bytes = value(&mut args)?.to_str().ok_or_else(|| invalid("invalid reserve"))?.parse().map_err(|_| invalid("invalid reserve"))?,
                Some("--check") if !check => check = true,
                _ => return Err(invalid("usage: openpilot-runtime-install --root ROOT [--check | --archive PATH --descriptor PATH] [--reserve-bytes BYTES]")),
            }
        }
        let offline = match (archive, descriptor) {
            (Some(a), Some(d)) if !check => Some((a, d)),
            (None, None) => None,
            _ => {
                return Err(invalid(
                    "--archive and --descriptor must be provided together without --check",
                ))
            }
        };
        Ok(Self {
            root: root.ok_or_else(|| invalid("--root is required"))?,
            check,
            offline,
            reserve_bytes,
        })
    }
}
fn value(args: &mut impl Iterator<Item = std::ffi::OsString>) -> Result<std::ffi::OsString> {
    args.next()
        .ok_or_else(|| invalid("argument requires a value"))
}
pub(crate) fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}

pub fn run(options: &Options, download: &impl Download) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let root = options.root.canonicalize()?;
    let sha = checkout_sha(&root)?;
    archive::real_directory(&root, Path::new("rust"))?;
    let flags = i32::try_from(rustix::fs::OFlags::NOFOLLOW.bits())
        .map_err(|_| invalid("unsupported no-follow flag"))?;
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .custom_flags(flags)
        .open(root.join("rust/.runtime-install.lock"))?;
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::LockExclusive)
        .map_err(std::io::Error::from)?;
    activation::recover(&root)?;
    if check_installed(&root, &sha).is_ok() {
        return Ok(());
    }
    if options.check {
        return check_installed(&root, &sha);
    }
    ensure_space(&root, options.reserve_bytes, MAX_JSON_BYTES)?;
    let packages = archive::real_directory(&root, Path::new("rust/.runtime-packages"))?;
    activation::clean_pending(&packages)?;
    activation::clean_orphans(&root)?;
    let staging = tempfile::Builder::new()
        .prefix(".pending-")
        .tempdir_in(&packages)?;
    let descriptor_path = staging.path().join(".runtime-release.json");
    let archive_path = staging.path().join(CACHED_ARCHIVE);
    let base = format!("https://github.com/bin9208/openpilot-rust/releases/download/runtime-{sha}");
    match &options.offline {
        Some((_, descriptor)) => copy_bounded(descriptor, &descriptor_path, MAX_JSON_BYTES)?,
        None => download.download(
            &format!("{base}/runtime-release.json"),
            &descriptor_path,
            MAX_JSON_BYTES,
        )?,
    }
    let descriptor: Descriptor = identity::read_json(&descriptor_path)?;
    descriptor.validate(&sha)?;
    let source_growth = descriptor.archive.unpacked_bytes.min(128 * 1024 * 1024);
    let growth = descriptor
        .archive
        .bytes
        .checked_add(descriptor.archive.unpacked_bytes)
        .and_then(|n| n.checked_add(source_growth))
        .ok_or_else(|| invalid("package size overflow"))?;
    ensure_space(&root, options.reserve_bytes, growth)?;
    match &options.offline {
        Some((archive, _)) => copy_bounded(archive, &archive_path, descriptor.archive.bytes)?,
        None => download.download(
            &format!("{base}/{}", descriptor.archive.name),
            &archive_path,
            descriptor.archive.bytes,
        )?,
    }
    if fs::metadata(&archive_path)?.len() != descriptor.archive.bytes
        || identity::digest(&archive_path)? != descriptor.archive.sha256
    {
        return Err(invalid("archive bytes or SHA-256 mismatch"));
    }
    archive::extract(&archive_path, staging.path(), &descriptor)?;
    // Keep exactly this generation's verified compressed package for offline
    // repair. The space reservation above already includes these bytes; normal
    // generation cleanup also retires the cache with its owning generation.
    identity::verify_generation(staging.path(), &descriptor)?;
    activation::activate(&root, staging, &descriptor)?;
    check_installed(&root, &sha)
}

/// Only reuse a complete, regular, exact-source archive; active executables may
/// be damaged, which is precisely why repair must not depend on their hashes.
pub(crate) fn repair_package(root: &Path, sha: &str) -> Option<(PathBuf, PathBuf)> {
    let read = || -> Result<(PathBuf, PathBuf)> {
        let generation = activation::active_generation(root)?;
        let descriptor_path = generation.join(".runtime-release.json");
        let archive_path = generation.join(CACHED_ARCHIVE);
        if !fs::symlink_metadata(&descriptor_path)?.is_file()
            || !fs::symlink_metadata(&archive_path)?.is_file()
        {
            return Err(invalid("offline repair cache must contain regular files"));
        }
        let descriptor: Descriptor = identity::read_json(&descriptor_path)?;
        descriptor.validate(sha)?;
        let expected_prefix = format!("gen-{sha}-{}-", descriptor.archive.sha256);
        if !generation
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.starts_with(&expected_prefix))
            || fs::metadata(&archive_path)?.len() != descriptor.archive.bytes
            || identity::digest(&archive_path)? != descriptor.archive.sha256
        {
            return Err(invalid("offline repair cache identity or digest mismatch"));
        }
        Ok((archive_path, descriptor_path))
    };
    match read() {
        Ok(package) => Some(package),
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            eprintln!("Local repair package unavailable: {error}");
            None
        }
    }
}

pub(crate) fn checkout_sha(root: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(root)
        .output()?;
    let sha = String::from_utf8(output.stdout)
        .map_err(|_| invalid("git returned non-UTF8 SHA"))?
        .trim()
        .to_owned();
    if !output.status.success() || !identity::hex(&sha, 40) {
        return Err(invalid("checkout has no valid full source SHA"));
    }
    Ok(sha)
}

fn check_installed(root: &Path, sha: &str) -> Result<()> {
    let manifest: Manifest = identity::read_json(&root.join("rust/runtime-manifest.json"))?;
    manifest.validate(sha)?;
    identity::verify_files(root, &manifest)?;
    if fs::symlink_metadata(root.join("rust/.runtime-active")).is_ok() {
        let generation = activation::active_generation(root)?;
        let descriptor: Descriptor =
            identity::read_json(&generation.join(".runtime-release.json"))?;
        descriptor.validate(sha)?;
        identity::verify_generation(&generation, &descriptor)?;
        for (path, expected) in [
            ("rust/bin", generation.join("rust/bin")),
            (
                "rust/runtime-manifest.json",
                generation.join("rust/runtime-manifest.json"),
            ),
        ] {
            if root.join(path).canonicalize()? != expected.canonicalize()? {
                return Err(invalid("runtime paths do not refer to active generation"));
            }
        }
    }
    Ok(())
}

fn ensure_space(root: &Path, reserve: u64, growth: u64) -> Result<()> {
    let stat = rustix::fs::statvfs(root).map_err(std::io::Error::from)?;
    let available = stat
        .f_bavail
        .checked_mul(stat.f_frsize)
        .ok_or_else(|| invalid("available space overflow"))?;
    let needed = reserve
        .checked_add(growth)
        .ok_or_else(|| invalid("required space overflow"))?;
    if available < needed {
        return Err(invalid(format!("insufficient disk space: {available} available, {needed} required; user data was not removed")));
    }
    Ok(())
}

pub(crate) fn copy_bounded(source: &Path, dest: &Path, max: u64) -> Result<()> {
    use std::io::Read;
    let mut input =
        File::open(source)?.take(max.checked_add(1).ok_or_else(|| invalid("size overflow"))?);
    let mut output = File::create(dest)?;
    if std::io::copy(&mut input, &mut output)? > max {
        return Err(invalid("input exceeds declared size limit"));
    }
    output.sync_all()?;
    Ok(())
}
