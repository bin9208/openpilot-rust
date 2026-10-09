use super::{Update, UpdateFailure};
use crate::{
    auto_update_pull::{events::ErrorEvent, Attempt, Failure, INFO_TIMEOUT},
    git_config, git_status, Error, Value,
};
use std::{
    fs::{File, OpenOptions},
    os::{fd::AsFd, unix::fs::OpenOptionsExt},
    sync::Arc,
};
use tokio::sync::watch;

impl Update {
    pub(super) async fn locked(
        &mut self,
        status: &git_status::Status,
        stopped: watch::Receiver<bool>,
    ) -> Result<(bool, bool, String), UpdateFailure> {
        let unchanged = || (false, false, String::new());
        let repository = self.pull.service.repository();
        let lock = Arc::new(
            OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .mode(0o600)
                .open(&repository.lock)
                .map_err(Error::from)?,
        );
        rustix::fs::flock(&*lock, rustix::fs::FlockOperation::NonBlockingLockExclusive).map_err(
            |_| {
                Failure::Busy(
                    "Build or another Git operation is running; retry when it finishes.".into(),
                )
            },
        )?;
        if !(self.policy.ready)() {
            return Ok(unchanged());
        }
        self.recovery
            .prepare(Arc::clone(&lock), stopped.clone())
            .await?;
        let commands = self
            .pull
            .service
            .locked_commands(Arc::clone(&lock), stopped.clone());
        let (code, branch) = commands
            .run(&["branch", "--show-current"], INFO_TIMEOUT)
            .await?;
        if code != 0 || branch != status.branch || !(self.policy.ready)() {
            return Ok(unchanged());
        }
        let (code, head) = commands.run(&["rev-parse", "HEAD"], INFO_TIMEOUT).await?;
        if code != 0 || Some(head.as_str()) != status.head.as_deref() || !(self.policy.ready)() {
            return Ok(unchanged());
        }
        let (code, output, target) = self.prepare(Arc::clone(&lock), stopped.clone()).await?;
        if code != 0 {
            self.pull.error(
                "pull_failed",
                ErrorEvent {
                    detail: output,
                    blocked: false,
                    fields: Value::object([]),
                },
            )?;
            return Ok(unchanged());
        }
        if !(self.policy.ready)() {
            return Ok(unchanged());
        }
        self.last_pull_at = (self.policy.monotonic)();
        Ok(self
            .pull
            .run(Attempt {
                target: &target,
                lock,
                stopped,
            })
            .await?)
    }

    async fn prepare(
        &self,
        lock: Arc<File>,
        mut stopped: watch::Receiver<bool>,
    ) -> Result<(i32, String, String), UpdateFailure> {
        let repository = self.pull.service.repository();
        let directory = repository.directory.clone();
        let launcher = repository.launcher.clone();
        let mut task = tokio::task::spawn_blocking(move || {
            git_config::prepare_git_pull(&git_config::Repository {
                directory: &directory,
                launcher: &launcher,
                lock: Some(lock.as_fd()),
            })
        });
        tokio::select! {
            result = &mut task => Ok(result??),
            () = async {
                while !*stopped.borrow_and_update() {
                    if stopped.changed().await.is_err() { break; }
                }
            } => {
                task.await??;
                Err(git_status::Failure::Cancelled.into())
            }
        }
    }
}
