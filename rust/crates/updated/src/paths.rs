use std::path::{Path, PathBuf};
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Paths {
    pub base: PathBuf,
    pub staging: PathBuf,
    pub lock: PathBuf,
    pub system_root: PathBuf,
}
impl Paths {
    pub fn for_runtime(base: PathBuf, system_root: PathBuf) -> Self {
        Self {
            base,
            staging: std::env::var_os("UPDATER_STAGING_ROOT")
                .map(PathBuf::from)
                .unwrap_or_else(|| system_root.join("data/safe_staging")),
            lock: std::env::var_os("UPDATER_LOCK_FILE")
                .map(PathBuf::from)
                .unwrap_or_else(|| system_root.join("tmp/safe_staging_overlay.lock")),
            system_root,
        }
    }
    pub fn merged(&self) -> PathBuf {
        self.staging.join("merged")
    }
    pub fn upper(&self) -> PathBuf {
        self.staging.join("upper")
    }
    pub fn metadata(&self) -> PathBuf {
        self.staging.join("metadata")
    }
    pub fn finalized(&self) -> PathBuf {
        self.staging.join("finalized")
    }
    pub fn overlay_init(&self) -> PathBuf {
        self.base.join(".overlay_init")
    }
    pub fn system(&self, path: &str) -> PathBuf {
        self.system_root.join(path.trim_start_matches('/'))
    }
    /// Production uses os.sync. Alternate roots restrict validation flushes to
    /// the caller-owned root while exercising the same flag/file operations.
    pub fn sync(&self) -> Result<(), std::io::Error> {
        if self.system_root == Path::new("/") {
            rustix::fs::sync();
            Ok(())
        } else {
            std::fs::File::open(&self.system_root)?.sync_all()
        }
    }
}
