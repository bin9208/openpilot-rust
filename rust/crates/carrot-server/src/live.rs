//! Active broker and WebSocket relay from carrot/server/live_runtime and carrot/realtime.
mod broker;
mod camera;
mod camera_frame;
pub mod compact;
mod compact_fields;
mod compact_schema;
pub mod http;
mod owner;
mod raw;
mod routes;
mod session;
mod snapshot;
mod value;

use crate::{
    web_sound::{socket, transport, Shutdown},
    Error, Value,
};
use openpilot_params::Params;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
};
use tokio::sync::{mpsc, oneshot, watch};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Single,
    Multiplex,
    Compact,
    Camera,
}
pub(super) struct Spec {
    mode: Mode,
    services: Vec<String>,
    hello: String,
}
pub(super) enum Command {
    Poll {
        force: bool,
        reply: oneshot::Sender<Result<Value, Error>>,
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
    engaged: watch::Receiver<bool>,
    quiescing: AtomicBool,
    count: Arc<Counter>,
    owner: Mutex<Option<JoinHandle<Result<(), Error>>>>,
}
impl Service {
    pub fn start(params: Option<Params>) -> Result<Arc<Self>, Error> {
        let (commands, receiver) = mpsc::channel(64);
        let (stop, stopped) = watch::channel(Shutdown::Running);
        let (engaged, signal) = watch::channel(false);
        let (changed, _) = watch::channel(());
        let owner = std::thread::Builder::new()
            .name("carrot-live".into())
            .spawn(move || {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?
                    .block_on(
                        tokio::task::LocalSet::new()
                            .run_until(owner::run(receiver, stopped, params, engaged)),
                    )
            })?;
        Ok(Arc::new(Self {
            commands,
            stop,
            engaged: signal,
            quiescing: AtomicBool::new(false),
            count: Arc::new(Counter {
                active: AtomicUsize::new(0),
                changed,
            }),
            owner: Mutex::new(Some(owner)),
        }))
    }
    pub fn engaged(&self) -> bool {
        *self.engaged.borrow()
    }
    pub fn is_idle(&self) -> bool {
        self.count.active.load(Ordering::SeqCst) == 0
    }
    pub fn subscribe(&self) -> watch::Receiver<()> {
        self.count.changed.subscribe()
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
    pub async fn snapshot(&self, force: bool) -> Result<Value, Error> {
        let (reply, result) = oneshot::channel();
        self.commands
            .send(Command::Poll { force, reply })
            .await
            .map_err(|_| Error::Source("realtime broker unavailable".into()))?;
        result
            .await
            .map_err(|_| Error::Source("realtime broker unavailable".into()))?
    }
    async fn launch(&self, upgrade: hyper::upgrade::OnUpgrade, spec: Spec) -> Result<(), Error> {
        if self.quiescing.load(Ordering::SeqCst) {
            return Err(Error::Source("realtime hub unavailable".into()));
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
            .map_err(|_| Error::Source("realtime hub unavailable".into()))
    }
    pub async fn shutdown(&self) -> Result<(), Error> {
        self.force();
        let owner = self
            .owner
            .lock()
            .map_err(|_| Error::Source("live owner lock poisoned".into()))?
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
        .map_err(|_| Error::Source("live owner panicked".into()))?
}
impl Drop for Service {
    fn drop(&mut self) {
        self.force();
        match self.owner.get_mut() {
            Ok(owner) => {
                if let Some(owner) = owner.take() {
                    if let Err(error) = join(owner) {
                        eprintln!("live cleanup: {error}");
                    }
                }
            }
            Err(error) => eprintln!("live cleanup: {error}"),
        }
    }
}
