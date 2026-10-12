//! One recovery-owned persistent shell; clients never own its process lifetime.
mod owner;
mod pty;
pub(crate) mod session;
mod shell;

use crate::{config::Config, websocket::Sender, Error};
use std::sync::{mpsc, Arc, Mutex};

type Reply<T> = mpsc::SyncSender<Result<T, Error>>;
enum Command {
    Ensure(Reply<bool>),
    Attach {
        client: Arc<Sender>,
        reset: bool,
        reply: Reply<u64>,
    },
    Detach {
        id: u64,
        reply: Reply<()>,
    },
    Write {
        bytes: Vec<u8>,
        reply: Reply<()>,
    },
    Resize {
        rows: i64,
        reply: Reply<()>,
    },
    Clear(Reply<()>),
    Stop(Reply<()>),
}

pub struct Terminal {
    commands: mpsc::SyncSender<Command>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl Terminal {
    pub fn new(config: Config) -> Result<Arc<Self>, Error> {
        rustix::process::set_child_subreaper(Some(rustix::process::Pid::INIT))
            .map_err(std::io::Error::from)?;
        let (commands, receiver) = mpsc::sync_channel(64);
        let worker = std::thread::Builder::new()
            .name("recovery-pty".into())
            .spawn(move || owner::run(config, receiver))?;
        Ok(Arc::new(Self {
            commands,
            worker: Mutex::new(Some(worker)),
        }))
    }
    fn request<T>(&self, command: impl FnOnce(Reply<T>) -> Command) -> Result<T, Error> {
        let (reply, result) = mpsc::sync_channel(1);
        self.commands
            .send(command(reply))
            .map_err(|_| Error::Source("terminal owner stopped".into()))?;
        result
            .recv()
            .map_err(|_| Error::Source("terminal owner stopped".into()))?
    }
    pub fn ensure(&self) -> Result<bool, Error> {
        self.request(Command::Ensure)
    }
    pub fn attach(&self, client: Arc<Sender>, reset: bool) -> Result<u64, Error> {
        self.request(|reply| Command::Attach {
            client,
            reset,
            reply,
        })
    }
    pub fn detach(&self, id: u64) -> Result<(), Error> {
        self.request(|reply| Command::Detach { id, reply })
    }
    pub fn write_text(&self, text: &str) -> Result<(), Error> {
        if text.is_empty() {
            return Ok(());
        }
        self.write(text.as_bytes())
    }
    pub fn write(&self, bytes: &[u8]) -> Result<(), Error> {
        self.request(|reply| Command::Write {
            bytes: bytes.into(),
            reply,
        })
    }
    pub fn clear_history(&self) -> Result<(), Error> {
        self.request(Command::Clear)
    }
    pub fn resize(&self, rows: i64) -> Result<(), Error> {
        self.request(|reply| Command::Resize { rows, reply })
    }
    pub fn shutdown(&self) -> Result<(), Error> {
        let mut worker = self
            .worker
            .lock()
            .map_err(|_| Error::Source("terminal worker lock poisoned".into()))?;
        if let Some(worker) = worker.take() {
            let result = self.request(Command::Stop);
            worker
                .join()
                .map_err(|_| Error::Source("terminal owner panicked".into()))?;
            result?;
        }
        Ok(())
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            eprintln!("[recovery] terminal shutdown: {error}");
        }
    }
}
