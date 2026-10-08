mod commands;
mod read;
mod status;

pub use status::{State, Status};
use std::{
    fs::OpenOptions,
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{watch, Mutex as AsyncMutex};

pub const CACHE_TTL: f64 = 600.0;
pub const POLL_INTERVAL: Duration = Duration::from_secs(60);
pub const INITIAL_DELAY: Duration = Duration::from_secs(8);
pub const GIT_TIMEOUT: Duration = Duration::from_secs(8);
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(25);

#[derive(Debug, thiserror::Error)]
pub enum Failure {
    #[error("Git status request cancelled")]
    Cancelled,
    #[error("Git status wall clock out of range")]
    Clock,
    #[error("Git status cache lock poisoned")]
    Cache,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Launch(#[from] openpilot_process_supervision::Error),
    #[error(transparent)]
    Task(#[from] tokio::task::JoinError),
}

pub struct Repository {
    pub directory: PathBuf,
    pub lock: PathBuf,
    pub launcher: PathBuf,
}

pub struct Service {
    repository: Repository,
    now: Arc<dyn Fn() -> f64 + Send + Sync>,
    cache: Mutex<Option<Status>>,
    refresh: Arc<AsyncMutex<()>>,
}

struct Cancellation(watch::Sender<bool>);
impl Drop for Cancellation {
    fn drop(&mut self) {
        self.0.send_replace(true);
    }
}

struct Completion(watch::Sender<bool>);
impl Drop for Completion {
    fn drop(&mut self) {
        self.0.send_replace(true);
    }
}

impl Service {
    pub fn original(launcher: PathBuf) -> Arc<Self> {
        Self::with_clock(
            Repository {
                directory: "/data/openpilot".into(),
                lock: std::env::var_os("CARROT_REPO_LOCK_PATH")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| "/tmp/carrot_repo_update.lock".into()),
                launcher,
            },
            || {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|t| t.as_secs_f64())
                    .unwrap_or_default()
            },
        )
    }

    pub fn with_clock(
        repository: Repository,
        now: impl Fn() -> f64 + Send + Sync + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            repository,
            now: Arc::new(now),
            cache: Mutex::new(None),
            refresh: Arc::new(AsyncMutex::new(())),
        })
    }

    fn fresh(&self) -> Result<Option<Status>, Failure> {
        Ok(self
            .cache
            .lock()
            .map_err(|_| Failure::Cache)?
            .as_ref()
            .filter(|status| {
                (self.now)()
                    - num_traits::ToPrimitive::to_f64(&status.checked_at).unwrap_or(f64::INFINITY)
                    < CACHE_TTL
            })
            .cloned())
    }

    pub fn clear_cache(&self) -> Result<(), Failure> {
        *self.cache.lock().map_err(|_| Failure::Cache)? = None;
        Ok(())
    }

    pub async fn wait_idle(&self) {
        let _transaction = self.refresh.lock().await;
    }

    pub async fn get(self: &Arc<Self>, force: bool) -> Result<Status, Failure> {
        self.get_with_completion(force, None).await
    }

    async fn get_with_completion(
        self: &Arc<Self>,
        force: bool,
        completion: Option<Completion>,
    ) -> Result<Status, Failure> {
        if !force {
            if let Some(status) = self.fresh()? {
                return Ok(status);
            }
        }
        let transaction = self.refresh.clone().lock_owned().await;
        if !force {
            if let Some(status) = self.fresh()? {
                return Ok(status);
            }
        }
        let file = Arc::new(
            OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .mode(0o600)
                .open(&self.repository.lock)?,
        );
        if rustix::fs::flock(&*file, rustix::fs::FlockOperation::NonBlockingLockExclusive).is_err()
        {
            let mut state =
                Status::error("Build or another Git operation is running", (self.now)())?;
            state.state = State::Busy;
            return Ok(state);
        }
        let (sender, stopped) = watch::channel(false);
        let cancellation = Cancellation(sender);
        let service = Arc::clone(self);
        let task = tokio::spawn(async move {
            let _completion = completion;
            let _transaction = transaction;
            let context = read::Context {
                service: &service,
                lock: file,
                stopped,
            };
            let status = context.read().await?;
            *service.cache.lock().map_err(|_| Failure::Cache)? = Some(status.clone());
            Ok::<_, Failure>(status)
        });
        let result = task.await?;
        drop(cancellation);
        result
    }

    pub async fn run_loop(
        self: &Arc<Self>,
        mut stopped: watch::Receiver<bool>,
    ) -> Result<(), Failure> {
        self.run_loop_with_timing(&mut stopped, POLL_INTERVAL, INITIAL_DELAY)
            .await
    }

    pub async fn run_loop_with_timing(
        self: &Arc<Self>,
        stopped: &mut watch::Receiver<bool>,
        interval: Duration,
        initial: Duration,
    ) -> Result<(), Failure> {
        if initial > Duration::ZERO {
            tokio::select! { _ = tokio::time::sleep(initial) => {}, _ = stopped.changed() => return Ok(()) }
        }
        loop {
            let (completion, mut completed) = watch::channel(false);
            tokio::select! {
                result = self.get_with_completion(true, Some(Completion(completion))) => {
                    if let Err(error) = result { eprintln!("[git_status] periodic check failed: {error}"); }
                }
                _ = stopped.changed() => {
                    completed.wait_for(|done| *done).await.map_err(std::io::Error::other)?;
                    return Ok(());
                }
            }
            tokio::select! { _ = tokio::time::sleep(interval) => {}, _ = stopped.changed() => return Ok(()) }
        }
    }
}
