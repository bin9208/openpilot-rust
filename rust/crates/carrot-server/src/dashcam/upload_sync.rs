mod owner;
mod state;
mod worker;

use super::{Failure, Service};
use crate::{Error, Value};
use openpilot_dashcam_upload::worker::Settings;
pub(super) use state::Admission;
pub use state::SyncState;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};
use tokio::{
    sync::{mpsc, oneshot, watch},
    task::JoinSet,
};

enum Owner {
    Running {
        commands: mpsc::Sender<owner::Start>,
        thread: Mutex<Option<thread::JoinHandle<()>>>,
    },
    Unavailable(String),
}
pub struct SyncUploads {
    owner: Owner,
    state: watch::Sender<SyncState>,
    queued: Mutex<JoinSet<()>>,
}
impl SyncUploads {
    pub fn original(service: &Service) -> Arc<Self> {
        let executable = std::env::current_exe().map_or_else(
            |_| PathBuf::from("openpilot-dashcam-upload"),
            |path| path.with_file_name("openpilot-dashcam-upload"),
        );
        Self::for_test(service.root.clone(), executable, None)
    }
    pub fn for_test(root: PathBuf, executable: PathBuf, settings: Option<Settings>) -> Arc<Self> {
        let state = state::channel();
        let stopped = state.subscribe();
        let config = Arc::new(owner::Config {
            root,
            executable,
            settings,
        });
        let (commands, receiver) = mpsc::channel(16);
        let owner = match thread::Builder::new()
            .name("dashcam-sync-upload-owner".into())
            .spawn(move || {
                match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime.block_on(owner::run(config, receiver, stopped)),
                    Err(error) => eprintln!("Dashcam sync owner runtime: {error}"),
                }
            }) {
            Ok(thread) => Owner::Running {
                commands,
                thread: Mutex::new(Some(thread)),
            },
            Err(error) => Owner::Unavailable(error.to_string()),
        };
        Arc::new(Self {
            owner,
            state,
            queued: Mutex::new(JoinSet::new()),
        })
    }
    pub(super) fn begin(&self) -> Result<Admission, Failure> {
        match &self.owner {
            Owner::Running { .. } => Admission::begin(&self.state).map_err(Failure::from),
            Owner::Unavailable(error) => Err(Error::Source(error.clone()).into()),
        }
    }
    pub(super) async fn upload(
        &self,
        admission: Admission,
        segments: Vec<String>,
    ) -> Result<Value, Failure> {
        let commands = match &self.owner {
            Owner::Running { commands, .. } => commands.clone(),
            Owner::Unavailable(error) => return Err(Error::Source(error.clone()).into()),
        };
        let (response, result) = oneshot::channel();
        {
            let mut queued = self
                .queued
                .lock()
                .map_err(|error| Error::Source(error.to_string()))?;
            while let Some(joined) = queued.try_join_next() {
                if let Err(error) = joined {
                    eprintln!("Dashcam sync queued call: {error}");
                }
            }
            queued.spawn(async move {
                if let Err(error) = commands
                    .send(owner::Start {
                        admission,
                        segments,
                        response,
                    })
                    .await
                {
                    drop(error.0.response.send(Err(
                        Error::Source("dashcam sync upload service stopped".into()).into(),
                    )));
                }
            });
        }
        result
            .await
            .map_err(|_| Error::Source("dashcam sync upload service stopped".into()))?
    }
    pub fn subscribe(&self) -> watch::Receiver<SyncState> {
        self.state.subscribe()
    }
    pub fn is_idle(&self) -> bool {
        self.state.borrow().active == 0
    }
    pub fn quiesce(&self) {
        self.state.send_modify(|state| {
            if state.phase == state::Phase::Running {
                state.phase = state::Phase::Quiescing;
            }
        });
    }
    pub fn force(&self) {
        self.state
            .send_modify(|state| state.phase = state::Phase::Force);
    }
    pub async fn shutdown(&self) -> Result<(), Error> {
        self.force();
        let owner = match &self.owner {
            Owner::Running { thread, .. } => thread
                .lock()
                .map_err(|error| Error::Source(error.to_string()))?
                .take(),
            Owner::Unavailable(_) => None,
        };
        if let Some(owner) = owner {
            tokio::task::spawn_blocking(move || owner.join())
                .await
                .map_err(|error| Error::Source(error.to_string()))?
                .map_err(|_| Error::Source("dashcam sync owner panicked".into()))?;
        }
        let mut queued = {
            let mut queued = self
                .queued
                .lock()
                .map_err(|error| Error::Source(error.to_string()))?;
            std::mem::take(&mut *queued)
        };
        while let Some(joined) = queued.join_next().await {
            joined.map_err(|error| Error::Source(error.to_string()))?;
        }
        Ok(())
    }
}
impl Drop for SyncUploads {
    fn drop(&mut self) {
        self.force();
        let queued = self
            .queued
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        queued.abort_all();
        match &mut self.owner {
            Owner::Running { thread, .. } => {
                if let Some(owner) = thread
                    .get_mut()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                {
                    if owner.join().is_err() {
                        eprintln!("Dashcam sync owner cleanup panicked");
                    }
                }
            }
            Owner::Unavailable(_) => {}
        }
    }
}
