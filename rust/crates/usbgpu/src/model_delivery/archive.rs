//! The verified upstream archive is retained as provenance, never run as Python.
use super::Error;
use flate2::read::GzDecoder;
use std::{
    fs::{self, File, Permissions},
    io,
    os::unix::fs::PermissionsExt,
    path::{Component, Path, PathBuf},
};

fn relative(path: &Path) -> Result<PathBuf, Error> {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => result.push(name),
            Component::RootDir | Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    return Err(Error::Invalid(
                        "runtime archive path escapes staging".into(),
                    ));
                }
            }
            Component::Prefix(_) => {
                return Err(Error::Invalid("unsupported archive path prefix".into()))
            }
        }
    }
    Ok(result)
}

/// Validate the complete archive before writing any member to staging.
pub fn validate(path: &Path) -> Result<(), Error> {
    let mut archive = tar::Archive::new(GzDecoder::new(File::open(path)?));
    let mut total = 0u64;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let kind = entry.header().entry_type();
        if !kind.is_file() && !kind.is_dir() {
            return Err(Error::Invalid(
                "runtime archive contains links/special files".into(),
            ));
        }
        total = total
            .checked_add(entry.header().size()?)
            .ok_or_else(|| Error::Invalid("runtime archive size overflow".into()))?;
        if total > 256 << 20 {
            return Err(Error::Invalid("runtime archive too large".into()));
        }
        relative(&entry.path()?)?;
        io::copy(&mut entry, &mut io::sink())?;
    }
    Ok(())
}

/// Install data files atomically after validation; no archive entry is executed.
pub fn install(path: &Path, destination: &Path) -> Result<(), Error> {
    validate(path)?;
    if destination.exists() {
        return Ok(());
    }
    let parent = destination
        .parent()
        .ok_or_else(|| Error::Invalid("runtime has no parent".into()))?;
    let staging = tempfile::tempdir_in(parent)?;
    let mut archive = tar::Archive::new(GzDecoder::new(File::open(path)?));
    for entry in archive.entries()? {
        let mut entry = entry?;
        let target = staging.path().join(relative(&entry.path()?)?);
        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(target)?;
        } else {
            let parent = target
                .parent()
                .ok_or_else(|| Error::Invalid("archive file has no parent".into()))?;
            fs::create_dir_all(parent)?;
            let mut output = File::create(&target)?;
            io::copy(&mut entry, &mut output)?;
            let mut mode = entry.header().mode()? & 0o755;
            if mode & 0o100 == 0 {
                mode &= !0o111;
            }
            mode |= 0o600;
            fs::set_permissions(&target, Permissions::from_mode(mode))?;
        }
    }
    std::fs::rename(staging.path(), destination)?;
    // TempDir cleanup sees the old staging name is absent after a successful rename.
    Ok(())
}
