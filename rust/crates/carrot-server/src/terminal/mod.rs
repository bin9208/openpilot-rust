//! Active local terminal and shared PTY from features/terminal.py and services/terminal_pty.py.
mod cli_http;
mod config;
mod frames;
pub mod http;
mod legacy;
mod owner;
mod protocol;
mod pty;
mod receive;
mod session;
mod state;
mod tmux;
use crate::{
    web_sound::{socket, Shutdown},
    Error, Value,
};
pub use config::Config;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use tokio::sync::{mpsc, oneshot, watch, Notify};

struct Client {
    sink: socket::Sink,
    finished: Arc<Notify>,
    closed: Arc<AtomicBool>,
}
enum Command {
    Capture {
        argv: Vec<String>,
        admission: Admission,
        reply: oneshot::Sender<Result<std::process::Output, crate::tools::runner::Failure>>,
    },
    Launch {
        upgrade: hyper::upgrade::OnUpgrade,
        spec: session::Spec,
        admission: Admission,
    },
    Snapshot(oneshot::Sender<Result<Value, Error>>),
    Attach {
        id: u64,
        client: Client,
        eligible: bool,
        reset: bool,
        reply: oneshot::Sender<Result<(), Error>>,
    },
    Detach(u64),
    Write {
        bytes: Vec<u8>,
        reply: oneshot::Sender<Result<(), Error>>,
    },
    Resize {
        rows: u16,
        reply: oneshot::Sender<Result<(), Error>>,
    },
    Clear(oneshot::Sender<Result<(), Error>>),
}
struct Counter {
    active: AtomicUsize,
    changed: watch::Sender<()>,
}
struct Admission(Arc<Counter>);
impl Drop for Admission {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
        self.0.changed.send_replace(());
    }
}
pub struct Service {
    pub config: Config,
    commands: mpsc::UnboundedSender<Command>,
    stop: watch::Sender<Shutdown>,
    count: Arc<Counter>,
    quiescing: AtomicBool,
    owner: Mutex<Option<std::thread::JoinHandle<Result<(), Error>>>>,
}
impl Service {
    pub fn start(config: Config) -> Result<Arc<Self>, Error> {
        let (commands, receiver) = mpsc::unbounded_channel();
        let (stop, stopped) = watch::channel(Shutdown::Running);
        let (changed, _) = watch::channel(());
        let owner_config = config.clone();
        let owner_commands = commands.clone();
        let owner = std::thread::Builder::new()
            .name("carrot-terminal".into())
            .spawn(move || {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?
                    .block_on(tokio::task::LocalSet::new().run_until(owner::run(
                        receiver,
                        owner_commands,
                        stopped,
                        owner_config,
                    )))
            })?;
        Ok(Arc::new(Self {
            config,
            commands,
            stop,
            count: Arc::new(Counter {
                active: AtomicUsize::new(0),
                changed,
            }),
            quiescing: AtomicBool::new(false),
            owner: Mutex::new(Some(owner)),
        }))
    }
    fn submit(&self, command: Command) -> Result<(), Error> {
        self.commands
            .send(command)
            .map_err(|_| Error::Source("terminal owner unavailable".into()))
    }
    fn admit_command(&self) -> Result<Admission, Error> {
        if self.quiescing.load(Ordering::SeqCst) {
            return Err(Error::Source("terminal owner unavailable".into()));
        }
        self.count.active.fetch_add(1, Ordering::SeqCst);
        self.count.changed.send_replace(());
        Ok(Admission(Arc::clone(&self.count)))
    }
    pub async fn snapshot(&self) -> Result<Value, Error> {
        let (reply, result) = oneshot::channel();
        self.submit(Command::Snapshot(reply))?;
        result
            .await
            .map_err(|_| Error::Source("terminal owner unavailable".into()))?
    }
    async fn capture(
        &self,
        argv: Vec<String>,
        admission: Admission,
    ) -> Result<std::process::Output, crate::tools::runner::Failure> {
        let (reply, result) = oneshot::channel();
        self.submit(Command::Capture {
            argv,
            admission,
            reply,
        })?;
        result
            .await
            .map_err(|_| Error::Source("terminal owner unavailable".into()))?
    }
    fn launch(&self, upgrade: hyper::upgrade::OnUpgrade, spec: session::Spec) -> Result<(), Error> {
        if self.quiescing.load(Ordering::SeqCst) {
            return Err(Error::Source("terminal owner unavailable".into()));
        }
        self.count.active.fetch_add(1, Ordering::SeqCst);
        self.count.changed.send_replace(());
        self.submit(Command::Launch {
            upgrade,
            spec,
            admission: Admission(Arc::clone(&self.count)),
        })
    }
    pub async fn write(&self, bytes: Vec<u8>) -> Result<(), Error> {
        let (reply, result) = oneshot::channel();
        self.submit(Command::Write { bytes, reply })?;
        result
            .await
            .map_err(|_| Error::Source("terminal owner unavailable".into()))?
    }
    pub fn is_idle(&self) -> bool {
        self.count.active.load(Ordering::SeqCst) == 0
    }
    pub fn changed(&self) -> watch::Receiver<()> {
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
    pub async fn shutdown(&self) -> Result<(), Error> {
        self.force();
        let owner = self
            .owner
            .lock()
            .map_err(|_| Error::Source("terminal owner lock poisoned".into()))?
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
        .map_err(|_| Error::Source("terminal owner panicked".into()))?
}
impl Drop for Service {
    fn drop(&mut self) {
        self.force();
        if let Ok(owner) = self.owner.get_mut() {
            if let Some(owner) = owner.take() {
                if let Err(error) = join(owner) {
                    eprintln!("Terminal fallback: {error}");
                }
            }
        }
    }
}
