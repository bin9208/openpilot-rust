use crate::Error;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

pub(super) fn suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(suffix);
    PathBuf::from(value)
}

pub(super) fn paths(path: &Path) -> Result<Vec<PathBuf>, Error> {
    let marker = suffix(path, ".chunkmanifest");
    if marker.is_file() {
        let mut bytes = String::new();
        File::open(marker)?.take(65).read_to_string(&mut bytes)?;
        let count: usize = bytes
            .trim()
            .parse()
            .map_err(|_| Error::Contract("invalid local chunk count"))?;
        if bytes.len() > 64 || count == 0 || count > 1024 {
            return Err(Error::Contract("local chunk count limit"));
        }
        Ok((1..=count)
            .map(|index| suffix(path, &format!(".chunk{index:02}of{count:02}")))
            .collect::<Vec<_>>())
    } else {
        Ok(vec![path.to_path_buf()])
    }
}

pub(super) fn fingerprint(path: &Path) -> Result<(u64, String), Error> {
    let mut hash = Sha256::new();
    let mut total = 0u64;
    for path in paths(path)? {
        let mut file = File::open(path)?;
        total = total
            .checked_add(file.metadata()?.len())
            .ok_or(Error::Contract("local artifact bytes overflow"))?;
        if total > 4 << 30 {
            return Err(Error::Contract("local artifact bytes limit"));
        }
        std::io::copy(&mut file, &mut hash)?;
    }
    Ok((total, format!("{:x}", hash.finalize())))
}
