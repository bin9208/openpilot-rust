use crate::{startup::BootBoundary, Error};
use std::{fs::File, path::PathBuf, thread::JoinHandle};

/// Native snapshot worker and launcher-owned repository lock. Dropping the
/// worker handle detaches it exactly as the source daemon thread does.
pub struct NativeBoot {
    pub params_directory: PathBuf,
    pub loggerd_directory: PathBuf,
    pub launcher: PathBuf,
    pub lock_path: PathBuf,
    pub lock: Option<File>,
    pub worker: Option<JoinHandle<Result<(), openpilot_bootlog::snapshot::Error>>>,
}
impl BootBoundary for NativeBoot {
    fn save_bootlog(&mut self) -> Result<(), Error> {
        self.worker = Some(openpilot_bootlog::snapshot::save_bootlog(
            &self.params_directory,
            &self.loggerd_directory,
            &self.launcher,
        )?);
        Ok(())
    }
    fn release_boot_lock(&mut self) -> Result<(), Error> {
        crate::boot_lock::release(&mut self.lock, &self.lock_path)
    }
}
