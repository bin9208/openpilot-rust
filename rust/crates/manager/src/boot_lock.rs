use crate::Error;
use std::{fs::File, os::unix::fs::MetadataExt, path::Path};

/// The native launcher transfers the inherited lock as an owned File. Validate
/// inode/device before unlocking; an identity error leaves ownership with caller.
/// Importing an integer descriptor from the legacy shell launcher is not done here.
pub fn release(lock: &mut Option<File>, path: &Path) -> Result<(), Error> {
    std::env::remove_var("CARROT_BOOT_LOCK_FD");
    if let Some(file) = lock {
        let actual = file.metadata()?;
        let expected = std::fs::metadata(path)?;
        if (actual.dev(), actual.ino()) != (expected.dev(), expected.ino()) {
            return Err(Error::Contract(
                "Unexpected boot repository lock descriptor",
            ));
        }
        file.unlock()?;
        drop(lock.take());
    }
    Ok(())
}
