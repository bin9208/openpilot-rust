//! Shared build-side estimated-growth admission. Device installation has a separate space policy.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub mod cli;
use serde::Serialize;
use std::path::Path;

pub const RESERVE_BYTES: u64 = 0;

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
            required_bytes: growth_bytes,
            growth_bytes,
        })
    }

    pub fn require(&self) -> Result<()> {
        if self.free_bytes < self.required_bytes {
            return Err(
                "insufficient disk: available space is below estimated operation growth".into(),
            );
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
    fn exact_estimated_growth_is_required() {
        let growth = 2 * 1024 * 1024 * 1024;
        let space = Space::new(growth, growth).unwrap();
        assert_eq!(space.required_bytes, growth);
        assert!(space.require().is_ok());
        assert!(Space::new(growth - 1, growth).unwrap().require().is_err());
    }

    #[test]
    fn zero_growth_needs_no_fixed_reserve() {
        let space = Space::new(0, 0).unwrap();
        assert_eq!(space.required_bytes, 0);
        assert!(space.require().is_ok());
        assert!(Space::new(0, 1).unwrap().require().is_err());
    }

    #[test]
    fn maximum_byte_growth_is_valid_and_requires_matching_capacity() {
        let space = Space::new(u64::MAX, u64::MAX).unwrap();
        assert_eq!(space.required_bytes, u64::MAX);
        assert!(space.require().is_ok());
        assert!(Space::new(u64::MAX - 1, u64::MAX)
            .unwrap()
            .require()
            .is_err());
    }

    #[test]
    fn nonexistent_destination_uses_existing_ancestor() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("new/nested/cache");
        let space = inspect(&destination, 1024).unwrap();
        assert_eq!(space.growth_bytes, 1024);
        assert_eq!(space.required_bytes, 1024);
        assert!(space.free_bytes > 0);
        assert!(!destination.exists());
    }
}
