use super::{
    admission::{Admission, Counter},
    engine_stream::OwnedClient,
    network::Endpoint,
};
use crate::{Error, Value};
use openpilot_params::Params;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    thread::JoinHandle,
};
use tokio::sync::{mpsc, oneshot, watch};

pub struct Config {
    pub state_path: PathBuf,
    pub secret_path: PathBuf,
    pub params: Option<Params>,
    pub endpoint: Endpoint,
}
pub(super) enum Command {
    Status(oneshot::Sender<Value>),
    Diagnostics(oneshot::Sender<Value>),
    GetKey(oneshot::Sender<Value>),
    SetKey(Value, oneshot::Sender<Result<Value, Error>>),
    ClearKey(oneshot::Sender<Result<Value, Error>>),
    Verify(Option<Value>, oneshot::Sender<Value>),
    Test(oneshot::Sender<Value>),
}
pub(super) struct Request {
    pub command: Command,
    pub admission: Admission,
}
pub struct Service {
    commands: mpsc::Sender<Request>,
    stop: watch::Sender<bool>,
    count: Arc<Counter>,
    queued: Mutex<Vec<tokio::task::JoinHandle<()>>>,
    owner: Mutex<Option<JoinHandle<Result<(), Error>>>>,
    owned: OwnedClient,
}
impl Service {
    pub fn original(
        config: &crate::config::Config,
        params: Option<Params>,
    ) -> Result<Arc<Self>, Error> {
        Self::start(Config {
            state_path: config.state.join("youtube_live.json"),
            secret_path: config.state.join("youtube_live_secret.json"),
            params,
            endpoint: Endpoint::default(),
        })
    }
    pub fn start(config: Config) -> Result<Arc<Self>, Error> {
        let (commands, receiver) = mpsc::channel(64);
        let (stop, stopped) = watch::channel(false);
        let owned = Arc::new(Mutex::new(None));
        let client = Arc::clone(&owned);
        let owner = std::thread::Builder::new()
            .name("carrot-youtube-live".into())
            .spawn(move || {
                let result = (|| {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()?;
                    runtime.block_on(
                        tokio::task::LocalSet::new()
                            .run_until(super::owner::run(config, receiver, stopped, client)),
                    )
                })();
                result
            })?;
        Ok(Arc::new(Self {
            commands,
            stop,
            count: Counter::new(),
            queued: Mutex::new(Vec::new()),
            owner: Mutex::new(Some(owner)),
            owned,
        }))
    }
    fn unavailable() -> Error {
        Error::Source("youtube live service unavailable".into())
    }
    pub async fn status(&self) -> Result<Value, Error> {
        let (reply, response) = oneshot::channel();
        self.submit(Command::Status(reply))?;
        response.await.map_err(|_| Self::unavailable())
    }
    pub async fn diagnostics(&self) -> Result<Value, Error> {
        let (reply, response) = oneshot::channel();
        self.submit(Command::Diagnostics(reply))?;
        response.await.map_err(|_| Self::unavailable())
    }
    pub async fn get_key(&self) -> Result<Value, Error> {
        let (reply, response) = oneshot::channel();
        self.submit(Command::GetKey(reply))?;
        response.await.map_err(|_| Self::unavailable())
    }
    pub async fn set_key(&self, key: Value) -> Result<Value, Error> {
        let (reply, response) = oneshot::channel();
        self.submit(Command::SetKey(key, reply))?;
        response.await.map_err(|_| Self::unavailable())?
    }
    pub async fn clear_key(&self) -> Result<Value, Error> {
        let (reply, response) = oneshot::channel();
        self.submit(Command::ClearKey(reply))?;
        response.await.map_err(|_| Self::unavailable())?
    }
    pub async fn verify(&self, value: Option<Value>) -> Result<Value, Error> {
        let (reply, response) = oneshot::channel();
        self.submit(Command::Verify(value, reply))?;
        response.await.map_err(|_| Self::unavailable())
    }
    pub async fn test(&self) -> Result<Value, Error> {
        let (reply, response) = oneshot::channel();
        self.submit(Command::Test(reply))?;
        response.await.map_err(|_| Self::unavailable())
    }
    fn submit(&self, command: Command) -> Result<(), Error> {
        let mut queued = self.queued.lock().map_err(|_| Self::unavailable())?;
        let admission = self.count.admit()?;
        queued.retain(|task| !task.is_finished());
        let sender = self.commands.clone();
        queued.push(tokio::spawn(async move {
            let _sent = sender.send(Request { command, admission }).await;
        }));
        Ok(())
    }
    pub fn quiesce(&self) {
        self.count.quiesce();
    }
    pub fn is_idle(&self) -> bool {
        self.count.idle()
    }
    pub fn changed(&self) -> watch::Receiver<()> {
        self.count.changed()
    }
    pub fn force(&self) {
        self.quiesce();
        self.stop.send_replace(true);
        if let Ok(queued) = self.queued.lock() {
            for task in queued.iter() {
                task.abort();
            }
        }
        if let Ok(client) = self.owned.lock() {
            if let Some(client) = client.as_ref() {
                client.wake_shutdown();
            }
        }
    }
    pub fn shutdown(&self) -> Result<(), Error> {
        self.quiesce();
        self.stop.send_replace(true);
        let owner = self.owner.lock().map_err(|_| Self::unavailable())?.take();
        match owner {
            Some(owner) => owner
                .join()
                .map_err(|_| Error::Source("YouTube service panicked".into()))?,
            None => Ok(()),
        }
    }
    pub async fn finish(self: &Arc<Self>) -> Result<(), Error> {
        let service = Arc::clone(self);
        tokio::task::spawn_blocking(move || service.shutdown())
            .await
            .map_err(|error| Error::Source(error.to_string()))?
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.force();
        let _closed = self.shutdown();
    }
}
