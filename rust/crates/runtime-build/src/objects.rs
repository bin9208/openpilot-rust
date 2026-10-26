//! Immutable copies of mutable build outputs, addressed by content and mode.
//! Only store-owned copies may be hardlinked into a candidate generation.
use crate::{digest_file, regular_file, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Object {
    pub sha256: String,
    pub bytes: u64,
    pub mode: u32,
}

#[derive(Debug, Serialize)]
pub struct Imported {
    pub object: Object,
    /// Bytes copied into the persistent object store. Zero on a verified hit.
    pub stored_bytes: u64,
}

fn directory(path: &Path) -> Result<()> {
    match fs::create_dir(path) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    if !fs::symlink_metadata(path)?.is_dir() {
        return Err("runtime object store requires real directories".into());
    }
    Ok(())
}

impl Object {
    fn directory(&self, store: &Path) -> Result<PathBuf> {
        if self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self.bytes == 0
            || self.bytes > 64 * 1024 * 1024 * 1024
            || !matches!(self.mode, 0o444 | 0o555)
        {
            return Err("invalid runtime content object".into());
        }
        Ok(store.join(format!("{}-{:o}", self.sha256, self.mode)))
    }

    pub fn verify(&self, store: &Path) -> Result<PathBuf> {
        if !fs::symlink_metadata(store)?.is_dir() {
            return Err("runtime object store is redirected".into());
        }
        let directory = self.directory(store)?;
        if !fs::symlink_metadata(&directory)?.is_dir() {
            return Err("runtime content object directory is redirected".into());
        }
        let path = directory.join("blob");
        let metadata = regular_file(&path)?.metadata()?;
        if metadata.len() != self.bytes
            || metadata.permissions().mode() & 0o7777 != self.mode
            || digest_file(&path)? != self.sha256
        {
            return Err("runtime content object failed size, mode or SHA-256 verification".into());
        }
        Ok(path)
    }

    /// Create a new candidate file without replacing an existing path. In
    /// particular, the caller cannot use this API to overwrite a running binary.
    pub fn materialize(&self, store: &Path, destination: &Path) -> Result<()> {
        let path = self.verify(store)?;
        fs::hard_link(path, destination)?;
        File::open(
            destination
                .parent()
                .ok_or("candidate file needs a parent")?,
        )?
        .sync_all()?;
        Ok(())
    }
}

/// Copy a build output into the store once. Never link the mutable Cargo output.
/// On cache hits no output bytes are copied. Space admission includes only this
/// copy's size plus the caller's operation-specific remaining growth estimate.
pub fn import(store: &Path, input: &Path, remaining_growth_bytes: u64) -> Result<Imported> {
    let mut input = regular_file(input)?;
    let metadata = input.metadata()?;
    if metadata.permissions().mode() & 0o7000 != 0
        || metadata.len() == 0
        || metadata.len() > 64 * 1024 * 1024 * 1024
    {
        return Err("invalid runtime output size or special permission bits".into());
    }
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    let mut bounded = (&mut input).take(metadata.len() + 1);
    let mut read = 0;
    loop {
        let size = bounded.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
        read += size as u64;
    }
    if read != metadata.len() {
        return Err("build output changed while identifying its content".into());
    }
    let object = Object {
        sha256: format!("{:x}", hash.finalize()),
        bytes: metadata.len(),
        mode: if metadata.permissions().mode() & 0o111 != 0 {
            0o555
        } else {
            0o444
        },
    };
    directory(store)?;
    let directory_path = object.directory(store)?;
    directory(&directory_path)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(directory_path.join("lock"))?;
    if !lock.metadata()?.is_file() {
        return Err("runtime content object lock is not a regular file".into());
    }
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive)?;
    let destination = directory_path.join("blob");
    if fs::symlink_metadata(&destination).is_ok() {
        object.verify(store)?;
        return Ok(Imported {
            object,
            stored_bytes: 0,
        });
    }
    let space = rustix::fs::statvfs(store)?;
    let available = space
        .f_bavail
        .checked_mul(space.f_frsize)
        .ok_or("runtime object space overflow")?;
    let growth = object
        .bytes
        .checked_add(remaining_growth_bytes)
        .ok_or("runtime object growth overflow")?;
    if available < growth {
        return Err(
            "insufficient space for runtime output copy; previous objects preserved".into(),
        );
    }
    let mut temporary = tempfile::NamedTempFile::new_in(&directory_path)?;
    input.seek(SeekFrom::Start(0))?;
    // Bound a concurrently growing input, then compare the copied bytes with the
    // identity read above. Any change fails before the object becomes visible.
    let copied = std::io::copy(&mut input.take(object.bytes + 1), &mut temporary)?;
    temporary.flush()?;
    if copied != object.bytes || digest_file(temporary.path())? != object.sha256 {
        return Err("build output changed while copying; no object published".into());
    }
    temporary
        .as_file()
        .set_permissions(fs::Permissions::from_mode(object.mode))?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(destination)?;
    File::open(&directory_path)?.sync_all()?;
    object.verify(store)?;
    Ok(Imported {
        stored_bytes: object.bytes,
        object,
    })
}
