use crate::Error;
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

pub(crate) fn read_verified(path: &Path, expected: &str, limit: usize) -> Result<Vec<u8>, Error> {
    let file = File::open(path)?;
    let limit = u64::try_from(limit).map_err(|_| Error::Limit("asset size"))?;
    if file.metadata()?.len() > limit {
        return Err(Error::Limit("asset size"));
    }
    let mut bytes = Vec::new();
    file.take(limit.checked_add(1).ok_or(Error::Limit("asset size"))?)
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).map_err(|_| Error::Limit("asset size"))? > limit {
        return Err(Error::Limit("asset size"));
    }
    if format!("{:x}", Sha256::digest(&bytes)) != expected {
        return Err(Error::Checksum("model asset"));
    }
    Ok(bytes)
}
