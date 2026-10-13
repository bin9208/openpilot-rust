//! Main-server support_terminal.py, support_tunnel.py and support_discord.py; the PTY owner is shared.
mod admission;
mod assets;
mod commands;
mod config;
mod context;
pub mod http;
mod lines;
mod local;
mod model;
mod network;
mod owner;
mod pipeline;
mod policy;
mod random;
mod receive;
mod session;
mod socket_input;
mod tunnel;
pub mod tunnel_binary;
mod webhook;

use crate::{diagnostic_metadata as metadata, web_sound::Shutdown, Error, Value};
use admission::{Admission, Counter};
pub use config::Config;
pub use policy::StartOptions;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot, watch};

enum Command {
    Quiesce(Admission),
    Start {
        options: StartOptions,
        admission: Admission,
        reply: oneshot::Sender<Result<Value, Error>>,
    },
    Stop {
        admission: Admission,
        reply: oneshot::Sender<Result<Value, Error>>,
    },
    Decision {
        id: String,
        approve: bool,
        admission: Admission,
        reply: oneshot::Sender<Result<Value, Error>>,
    },
    Snapshot(oneshot::Sender<Value>),
    Launch {
        upgrade: hyper::upgrade::OnUpgrade,
        mode: session::Mode,
        admission: Admission,
    },
}
pub struct Service {
    pub config: Config,
    commands: mpsc::UnboundedSender<Command>,
    stop: watch::Sender<Shutdown>,
    count: Arc<Counter>,
    owner: Mutex<Option<std::thread::JoinHandle<Result<(), Error>>>>,
}
impl Service {
    pub fn start(config: Config) -> Result<Arc<Self>, Error> {
        let (commands, received) = mpsc::unbounded_channel();
        let (stop, stopped) = watch::channel(Shutdown::Running);
        let count = Counter::new();
        let owned_config = config.clone();
        let owned_count = Arc::clone(&count);
        let owner = std::thread::Builder::new()
            .name("carrot-support".into())
            .spawn(move || {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?
                    .block_on(tokio::task::LocalSet::new().run_until(owner::run(
                        received,
                        stopped,
                        owned_config,
                        owned_count,
                    )))
            })?;
        Ok(Arc::new(Self {
            config,
            commands,
            stop,
            count,
            owner: Mutex::new(Some(owner)),
        }))
    }
    fn submit(&self, command: Command) -> Result<(), Error> {
        self.commands
            .send(command)
            .map_err(|_| Error::Source("support terminal unavailable".into()))
    }
    pub async fn start_session(&self, options: StartOptions) -> Result<Value, Error> {
        let (reply, result) = oneshot::channel();
        self.submit(Command::Start {
            options,
            admission: self.count.admit()?,
            reply,
        })?;
        result
            .await
            .map_err(|_| Error::Source("support terminal unavailable".into()))?
    }
    pub async fn stop_session(&self) -> Result<Value, Error> {
        let (reply, result) = oneshot::channel();
        self.submit(Command::Stop {
            admission: self.count.admit()?,
            reply,
        })?;
        result
            .await
            .map_err(|_| Error::Source("support terminal unavailable".into()))?
    }
    pub async fn decision(&self, id: String, approve: bool) -> Result<Value, Error> {
        let (reply, result) = oneshot::channel();
        self.submit(Command::Decision {
            id,
            approve,
            admission: self.count.admit()?,
            reply,
        })?;
        result
            .await
            .map_err(|_| Error::Source("support terminal unavailable".into()))?
    }
    pub async fn snapshot(&self) -> Result<Value, Error> {
        let (reply, result) = oneshot::channel();
        self.submit(Command::Snapshot(reply))?;
        result
            .await
            .map_err(|_| Error::Source("support terminal unavailable".into()))
    }
    fn launch(&self, upgrade: hyper::upgrade::OnUpgrade, mode: session::Mode) -> Result<(), Error> {
        self.submit(Command::Launch {
            upgrade,
            mode,
            admission: self.count.admit()?,
        })
    }
    pub fn is_idle(&self) -> bool {
        self.count.idle()
    }
    pub fn changed(&self) -> watch::Receiver<()> {
        self.count.changed()
    }
    pub fn quiesce(&self) {
        if self.count.quiesce() {
            if let Err(error) = self.submit(Command::Quiesce(self.count.owned())) {
                eprintln!("Support quiesce: {error}");
            }
        }
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
            .map_err(|_| Error::Source("support owner poisoned".into()))?
            .take();
        if let Some(owner) = owner {
            tokio::task::spawn_blocking(move || join(owner))
                .await
                .map_err(|error| Error::Source(error.to_string()))??;
        }
        Ok(())
    }
}
fn join(owner: std::thread::JoinHandle<Result<(), Error>>) -> Result<(), Error> {
    owner
        .join()
        .map_err(|_| Error::Source("support owner panicked".into()))?
}
impl Drop for Service {
    fn drop(&mut self) {
        self.force();
        if let Ok(owner) = self.owner.get_mut() {
            if let Some(owner) = owner.take() {
                if let Err(error) = join(owner) {
                    eprintln!("Support owner fallback: {error}");
                }
            }
        }
    }
}
