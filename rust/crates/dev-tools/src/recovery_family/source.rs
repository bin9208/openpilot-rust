use super::Result;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

/// Held, bounded regular source or ELF admission. A changed path is rejected
/// without opening a replacement FIFO or following a replacement symlink.
pub struct Pin {
    path: PathBuf,
    held: File,
    maximum: u64,
    identity: (u64, u64, u64),
    digest: String,
}

impl Pin {
    pub fn open(path: &Path, maximum: u64) -> Result<Self> {
        let held = File::from(rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        let metadata = held.metadata()?;
        if !metadata.is_file() || metadata.len() > maximum {
            return Err("invalid recovery family input".into());
        }
        let identity = (metadata.dev(), metadata.ino(), metadata.len());
        let digest = hash_held(&held, maximum, metadata.len())?;
        Ok(Self {
            path: path.to_owned(),
            held,
            maximum,
            identity,
            digest,
        })
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }

    pub fn verify(&self) -> Result<()> {
        let held = self.held.metadata()?;
        let path = fs::symlink_metadata(&self.path)?;
        let identity =
            |metadata: &std::fs::Metadata| (metadata.dev(), metadata.ino(), metadata.len());
        if !path.is_file()
            || path.file_type().is_symlink()
            || identity(&held) != self.identity
            || identity(&path) != self.identity
            || hash_held(&self.held, self.maximum, self.identity.2)? != self.digest
        {
            return Err("recovery family input changed".into());
        }
        Ok(())
    }
}

fn hash_held(file: &File, maximum: u64, expected_len: u64) -> Result<String> {
    let mut clone = file.try_clone()?;
    // try_clone shares the seek position. The descriptor is never otherwise
    // read after binding, so rewind it explicitly before every comparison.
    use std::io::Seek;
    clone.rewind()?;
    let mut limited = clone.take(maximum + 1);
    let mut hash = Sha256::new();
    let mut length = 0_u64;
    let mut chunk = [0_u8; 65_536];
    loop {
        let count = limited.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        length += count as u64;
        hash.update(&chunk[..count]);
    }
    if length != expected_len || length > maximum {
        return Err("recovery family input length changed".into());
    }
    Ok(format!("{:x}", hash.finalize()))
}
