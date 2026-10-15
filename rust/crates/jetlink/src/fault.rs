//! Short-lived source-compatible disengagement notice after active link loss.
use crate::Error;
use std::{io::Write, path::Path};
pub fn write(path: &Path, monotonic_seconds: f64) -> Result<(), Error> {
    if !monotonic_seconds.is_finite() || monotonic_seconds < 0. {
        return Err(Error::Contract("fault timestamp"));
    }
    let parent = path.parent().ok_or(Error::Contract("fault path"))?;
    let mut staging = tempfile::NamedTempFile::new_in(parent)?;
    write!(staging, "{monotonic_seconds:.17}")?;
    staging.flush()?;
    staging.persist(path).map_err(|error| error.error)?;
    Ok(())
}
