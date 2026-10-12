use super::owner;
use crate::{web_sound::Shutdown, Error, Value};
use openpilot_params::Params;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
};
use tokio::sync::{mpsc, oneshot, watch};

pub(super) enum Mode {
    State { takeover: bool },
    Media { include_map: bool, hud: bool },
}
pub(super) struct Spec {
    pub identity: String,
    pub mode: Mode,
}
pub(super) enum Command {
    Allowed(oneshot::Sender<bool>),
    Status(oneshot::Sender<Value>),
    Diagnostic {
        peer: String,
        value: Value,
        reply: oneshot::Sender<()>,
    },
    Launch {
        upgrade: hyper::upgrade::OnUpgrade,
        spec: Spec,
        admission: Admission,
    },
}
struct Counter {
    active: AtomicUsize,
    changed: watch::Sender<()>,
}
pub(super) struct Admission(Arc<Counter>);
impl Drop for Admission {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
        self.0.changed.send_replace(());
    }
}
pub struct Service {
    commands: mpsc::Sender<Command>,
    stop: watch::Sender<Shutdown>,
    quiescing: AtomicBool,
    count: Arc<Counter>,
    owner: Mutex<Option<JoinHandle<Result<(), Error>>>>,
}
impl Service {
    pub fn start(params: Option<Params>) -> Result<Arc<Self>, Error> {
        let (commands, receiver) = mpsc::channel(64);
        let (stop, stopped) = watch::channel(Shutdown::Running);
        let (changed, _) = watch::channel(());
        let owner = std::thread::Builder::new()
            .name("carrot-navi-web".into())
            .spawn(move || {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?
                    .block_on(
                        tokio::task::LocalSet::new()
                            .run_until(owner::run(receiver, stopped, params)),
                    )
            })?;
        Ok(Arc::new(Self {
            commands,
            stop,
            quiescing: AtomicBool::new(false),
            count: Arc::new(Counter {
                active: AtomicUsize::new(0),
                changed,
            }),
            owner: Mutex::new(Some(owner)),
        }))
    }
    fn unavailable() -> Error {
        Error::Source("Carrot Navi web bridge unavailable".into())
    }
    pub async fn allowed(&self) -> Result<bool, Error> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Allowed(reply))
            .await
            .map_err(|_| Self::unavailable())?;
        response.await.map_err(|_| Self::unavailable())
    }
    pub async fn status(&self) -> Result<Value, Error> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Status(reply))
            .await
            .map_err(|_| Self::unavailable())?;
        response.await.map_err(|_| Self::unavailable())
    }
    pub async fn diagnostic(&self, peer: String, value: Value) -> Result<(), Error> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Diagnostic { peer, value, reply })
            .await
            .map_err(|_| Self::unavailable())?;
        response.await.map_err(|_| Self::unavailable())
    }
    pub(super) async fn launch(
        &self,
        upgrade: hyper::upgrade::OnUpgrade,
        spec: Spec,
    ) -> Result<(), Error> {
        if self.quiescing.load(Ordering::SeqCst) {
            return Err(Self::unavailable());
        }
        self.count.active.fetch_add(1, Ordering::SeqCst);
        self.count.changed.send_replace(());
        self.commands
            .send(Command::Launch {
                upgrade,
                spec,
                admission: Admission(Arc::clone(&self.count)),
            })
            .await
            .map_err(|_| Self::unavailable())
    }
    pub fn subscribe(&self) -> watch::Receiver<()> {
        self.count.changed.subscribe()
    }
    pub fn is_idle(&self) -> bool {
        self.count.active.load(Ordering::SeqCst) == 0
    }
    pub fn quiesce(&self) {
        self.quiescing.store(true, Ordering::SeqCst);
        self.stop.send_if_modified(|state| {
            if *state == Shutdown::Running {
                *state = Shutdown::Quiescing;
                true
            } else {
                false
            }
        });
    }
    pub fn force(&self) {
        self.quiesce();
        self.stop.send_replace(Shutdown::Force);
    }
    pub async fn shutdown(&self) -> Result<(), Error> {
        self.force();
        let owner = self
            .owner
            .lock()
            .map_err(|_| Error::Source("Navi owner lock poisoned".into()))?
            .take();
        if let Some(owner) = owner {
            tokio::task::spawn_blocking(move || join(owner))
                .await
                .map_err(|error| Error::Source(error.to_string()))??;
        }
        Ok(())
    }
}
fn join(owner: JoinHandle<Result<(), Error>>) -> Result<(), Error> {
    owner
        .join()
        .map_err(|_| Error::Source("Navi owner panicked".into()))?
}
impl Drop for Service {
    fn drop(&mut self) {
        self.force();
        match self.owner.get_mut() {
            Ok(owner) => {
                if let Some(owner) = owner.take() {
                    if let Err(error) = join(owner) {
                        eprintln!("Navi cleanup: {error}");
                    }
                }
            }
            Err(error) => eprintln!("Navi cleanup: {error}"),
        }
    }
}
