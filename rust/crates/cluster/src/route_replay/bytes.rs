use super::Error;
use std::{fs, path::Path};

/// # Errors
/// Returns file-read or compressed-stream decoding errors.
pub fn read_log_bytes(path: &Path) -> Result<Vec<u8>, Error> {
    let data = fs::read(path)?;
    if path.extension().is_some_and(|suffix| suffix == "bz2") || data.starts_with(b"BZh") {
        Ok(super::codec::decode_bzip(&data)?)
    } else if path.extension().is_some_and(|suffix| suffix == "zst")
        || data.starts_with(&[0x28, 0xb5, 0x2f, 0xfd])
    {
        Ok(super::codec::decode_zstd(&data)?)
    } else {
        Ok(data)
    }
}
