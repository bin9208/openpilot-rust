//! Conservative index.lock recovery from common/repo_update.py and async_process.py.
mod lookup;
mod processes;

use crate::{Error, git_status::Repository};
use num_traits::ToPrimitive;
use std::{
    fs::{self, File, Metadata},
    os::unix::fs::MetadataExt,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::watch;

pub struct Inspection {
    pub proc_root: PathBuf,
    pub now: Arc<dyn Fn() -> f64 + Send + Sync>,
    pub pause: Arc<dyn Fn(Duration) -> Result<(), Error> + Send + Sync>,
}

impl Default for Inspection {
    fn default() -> Self {
        Self {
            proc_root: "/proc".into(),
            now: Arc::new(|| match SystemTime::now().duration_since(UNIX_EPOCH) {
                Ok(time) => time.as_secs_f64(),
                Err(time) => -time.duration().as_secs_f64(),
            }),
            pause: Arc::new(|duration| {
                std::thread::sleep(duration);
                Ok(())
            }),
        }
    }
}

pub struct Recovery {
    repository: Repository,
    inspection: Inspection,
}

#[derive(Debug, thiserror::Error)]
pub enum Failure {
    #[error("{0}")]
    Busy(String),
    #[error(transparent)]
    Runtime(#[from] Error),
    #[error(transparent)]
    Decode(Error),
    #[error("{0}")]
    Timeout(String),
    #[error("repository inspection cancelled")]
    Cancelled,
    #[error(transparent)]
    Task(#[from] tokio::task::JoinError),
}

fn identity(metadata: &Metadata) -> (u64, u64, i64, i64, u64) {
    (
        metadata.dev(),
        metadata.ino(),
        metadata.mtime(),
        metadata.mtime_nsec(),
        metadata.size(),
    )
}

impl Recovery {
    pub fn new(repository: Repository) -> Arc<Self> {
        Self::with_inspection(repository, Inspection::default())
    }

    pub fn with_inspection(repository: Repository, inspection: Inspection) -> Arc<Self> {
        Arc::new(Self {
            repository,
            inspection,
        })
    }

    /// The blocking inspection retains the already-held lock until completion, including cancellation.
    pub async fn prepare(
        self: &Arc<Self>,
        lock: Arc<File>,
        mut stopped: watch::Receiver<bool>,
    ) -> Result<bool, Failure> {
        let recovery = Arc::clone(self);
        let mut task = tokio::task::spawn_blocking(move || recovery.recover(&lock));
        tokio::select! {
            result = &mut task => result?,
            () = async {
                while !*stopped.borrow_and_update() {
                    if stopped.changed().await.is_err() {
                        break;
                    }
                }
            } => {
                task.await??;
                Err(Failure::Cancelled)
            }
        }
    }

    fn recover(&self, lock: &File) -> Result<bool, Failure> {
        let path = lookup::index_path(&self.repository, lock)?;
        let inspect = || match fs::symlink_metadata(&path) {
            Ok(metadata) => Ok(Some(metadata)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(crate::state::io_error(error, &path)),
        };
        let Some(before) = inspect()? else {
            return Ok(false);
        };
        if !before.file_type().is_file() {
            return Err(Failure::Busy(
                "Git index lock is not a regular file; manual inspection required.".into(),
            ));
        }
        let mtime = before.mtime().to_f64().unwrap_or(f64::INFINITY)
            + before.mtime_nsec().to_f64().unwrap_or(0.) / 1_000_000_000.;
        if (self.inspection.now)() - mtime < 60.
            || processes::git_running(&self.inspection.proc_root)?
        {
            return Err(Failure::Busy(
                "Git index is in use; waiting before retrying.".into(),
            ));
        }
        (self.inspection.pause)(Duration::from_millis(100))?;
        let Some(after) = inspect()? else {
            return Ok(false);
        };
        if identity(&before) != identity(&after)
            || processes::git_running(&self.inspection.proc_root)?
        {
            return Err(Failure::Busy(
                "Git index lock changed or Git is running; retry later.".into(),
            ));
        }
        fs::remove_file(&path).map_err(|error| crate::state::io_error(error, &path))?;
        println!("[repo_update] removed abandoned index.lock after checking Git processes");
        Ok(true)
    }
}
