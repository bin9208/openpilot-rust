use super::{Result, SystemStats};
use std::path::{Path, PathBuf};
mod average;
mod files;
mod numeric;
pub use numeric::MemoryBytes;

pub struct LinuxStatsReader {
    root: PathBuf,
    previous: Option<files::CpuTimes>,
}
impl LinuxStatsReader {
    #[must_use]
    pub fn new(proc_root: &Path) -> Self {
        Self {
            root: proc_root.into(),
            previous: None,
        }
    }
    /// # Errors
    /// Returns a numeric conversion failure while retaining the previous CPU sample.
    pub fn sample(&mut self) -> Result<Option<SystemStats>> {
        if !self.root.join("stat").exists() && !self.root.join("meminfo").exists() {
            return Ok(None);
        }
        files::sample(&self.root, &mut self.previous).map(Some)
    }
}
