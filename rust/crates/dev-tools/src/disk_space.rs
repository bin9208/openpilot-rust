//! Shared build-side reserve. Device installation has a separate space policy.
use crate::seal::Result;
use serde::Serialize;
use std::path::Path;

pub const RESERVE_BYTES: u64 = 25 * 1024 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct Space {
    pub free_bytes: u64,
    pub required_bytes: u64,
    pub growth_bytes: u64,
}

impl Space {
    pub fn new(free_bytes: u64, growth_bytes: u64) -> Result<Self> {
        Ok(Self {
            free_bytes,
            required_bytes: RESERVE_BYTES
                .checked_add(growth_bytes)
                .ok_or("disk reserve plus growth exceeds u64")?,
            growth_bytes,
        })
    }

    pub fn require(&self) -> Result<()> {
        if self.free_bytes < self.required_bytes {
            return Err("insufficient disk: retain 25 GiB plus operation growth; recover at least 35 GiB before resuming".into());
        }
        Ok(())
    }
}

pub fn inspect(destination: &Path, growth: u64) -> Result<Space> {
    let mut path = std::env::current_dir()?.join(destination);
    while !path.try_exists()? {
        if !path.pop() {
            return Err("destination ancestor missing".into());
        }
    }
    let stat = rustix::fs::statvfs(&path)?;
    Space::new(stat.f_bavail.saturating_mul(stat.f_frsize), growth)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_reserve_and_growth_are_required_without_overflow() {
        let growth = 2 * 1024 * 1024 * 1024;
        assert!(Space::new(RESERVE_BYTES + growth, growth)
            .unwrap()
            .require()
            .is_ok());
        assert!(Space::new(RESERVE_BYTES + growth - 1, growth)
            .unwrap()
            .require()
            .is_err());
        assert!(Space::new(u64::MAX, u64::MAX).is_err());
        assert!(Space::new(RESERVE_BYTES, 0).unwrap().require().is_ok());
    }

    #[test]
    fn nonexistent_destination_uses_existing_ancestor() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("new/nested/cache");
        let space = inspect(&destination, 1024).unwrap();
        assert_eq!(space.growth_bytes, 1024);
        assert_eq!(space.required_bytes, RESERVE_BYTES + 1024);
        assert!(space.free_bytes > 0);
        assert!(!destination.exists());
    }
}
