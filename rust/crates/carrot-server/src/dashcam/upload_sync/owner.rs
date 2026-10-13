use super::{state, Admission, SyncState};
use crate::{dashcam::Failure, Error, Value};
use openpilot_dashcam_upload::catalog;
use openpilot_dashcam_upload::worker::Settings;
use std::{path::PathBuf, sync::Arc};
use tokio::{
    sync::{mpsc, oneshot, watch},
    task::JoinSet,
};

pub(super) struct Config {
    pub root: PathBuf,
    pub executable: PathBuf,
    pub settings: Option<Settings>,
}
pub(super) struct Start {
    pub admission: Admission,
    pub segments: Vec<String>,
    pub response: oneshot::Sender<Result<Value, Failure>>,
}
fn reply(start: Start, result: Result<Value, Failure>) {
    // A disconnected HTTP receiver does not cancel the owned upload.
    drop(start.response.send(result));
    drop(start.admission);
}
pub(super) async fn run(
    config: Arc<Config>,
    mut commands: mpsc::Receiver<Start>,
    mut stopped: watch::Receiver<SyncState>,
) {
    let mut workers = JoinSet::new();
    loop {
        if stopped.borrow().phase == state::Phase::Force {
            break;
        }
        tokio::select! {
            command = commands.recv() => {
                let Some(start) = command else { break };
                let config = Arc::clone(&config);
                let stopped = stopped.clone();
                workers.spawn(async move {
                    let result = match catalog::validate_selection(&config.root, &start.segments) {
                        Ok(segments) => super::worker::run(config, segments, stopped).await.map_err(Failure::from),
                        Err(openpilot_dashcam_upload::Error::Http { status, text }) => Err(Failure::Http { status, message: text }),
                        Err(error) => Err(Error::Source(error.to_string()).into()),
                    };
                    (start, result)
                });
            }
            Some(joined) = workers.join_next(), if !workers.is_empty() => {
                match joined {
                    Ok((start, result)) => reply(start, result),
                    Err(error) => eprintln!("Dashcam sync worker task: {error}"),
                }
            }
            _ = stopped.changed() => {}
        }
    }
    commands.close();
    while let Ok(start) = commands.try_recv() {
        reply(
            start,
            Err(Error::Source("dashcam sync upload service stopped".into()).into()),
        );
    }
    drop(commands);
    while let Some(joined) = workers.join_next().await {
        match joined {
            Ok((start, result)) => reply(start, result),
            Err(error) => eprintln!("Dashcam sync worker cleanup: {error}"),
        }
    }
}
