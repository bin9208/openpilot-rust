use super::{
    frames::{output, send},
    pty::Generation,
    tmux, Client, Config,
};
use crate::{web_sound::Shutdown, Error, Value};
use std::{collections::BTreeMap, sync::atomic::Ordering};
use tokio::sync::watch;

pub(super) struct State {
    pub generation: Option<Generation>,
    pub clients: BTreeMap<u64, Client>,
    pub primary: Option<u64>,
    pub history: Vec<u8>,
    pub rows: u16,
}
pub(super) struct Attachment {
    pub id: u64,
    pub client: Client,
    pub eligible: bool,
    pub reset: bool,
}
pub(super) struct Shell<'a> {
    pub config: &'a Config,
    pub runner: &'a crate::tools::runner::Runner,
}
impl State {
    pub fn new() -> Self {
        Self {
            generation: None,
            clients: BTreeMap::new(),
            primary: None,
            history: Vec::new(),
            rows: 30,
        }
    }
    pub fn snapshot(&mut self) -> Result<Value, Error> {
        let alive = self
            .generation
            .as_mut()
            .map(Generation::alive)
            .transpose()?
            .unwrap_or(false);
        Ok(Value::object([
            ("ok", Value::Bool(true)),
            ("alive", Value::Bool(alive)),
            (
                "pid",
                self.generation
                    .as_ref()
                    .filter(|_| alive)
                    .map_or(Value::Null, |generation| {
                        Value::integer(generation.pid.as_raw_nonzero().get())
                    }),
            ),
            ("clients", Value::integer(self.clients.len())),
            (
                "primary",
                Value::Bool(self.primary.is_some_and(|id| {
                    self.clients
                        .get(&id)
                        .is_some_and(|client| !client.closed.load(Ordering::SeqCst))
                })),
            ),
            ("rows", Value::integer(self.rows)),
            ("cols", Value::integer(100)),
            ("history_bytes", Value::integer(self.history.len())),
            ("session", Value::text("login-shell")),
        ]))
    }
    pub async fn attach(
        &mut self,
        shell: Shell<'_>,
        attachment: Attachment,
        stop: &mut watch::Receiver<Shutdown>,
    ) -> Result<(), Error> {
        let Attachment {
            id,
            client,
            eligible,
            reset,
        } = attachment;
        if reset {
            self.finish(Value::Null, stop).await;
            self.history.clear();
        }
        let alive = self
            .generation
            .as_mut()
            .map(Generation::alive)
            .transpose()?
            .unwrap_or(false);
        if !alive {
            self.generation.take();
            self.history.clear();
            self.rows = 30;
            let command = tmux::start_command(shell.config, shell.runner).await?;
            self.generation = Some(Generation::start(shell.config, &command)?);
        }
        let pid = self
            .generation
            .as_ref()
            .ok_or_else(|| Error::Source("terminal session is not running".into()))?
            .pid
            .as_raw_nonzero()
            .get();
        self.clients.insert(id, client);
        if eligible
            && self.primary.is_none_or(|primary| {
                self.clients
                    .get(&primary)
                    .is_none_or(|client| client.closed.load(Ordering::SeqCst))
            })
        {
            self.primary = Some(id);
        }
        let meta = Value::object([
            ("type", Value::text("meta")),
            ("mode", Value::text("pty")),
            ("session", Value::text("login-shell")),
            ("created", Value::Bool(!alive)),
            ("user", Value::text("comma")),
            ("pid", Value::integer(pid)),
            ("clients", Value::integer(self.clients.len())),
            ("primary", Value::Bool(self.primary == Some(id))),
            ("rows", Value::integer(self.rows)),
            ("cols", Value::integer(100)),
        ]);
        if let Some(client) = self.clients.get(&id) {
            send(client, meta, stop).await?;
            if !self.history.is_empty() {
                send(client, output(&self.history, true), stop).await?;
            }
        }
        Ok(())
    }
    pub fn detach(&mut self, id: u64) {
        self.clients.remove(&id);
        if self.primary == Some(id) {
            self.primary = self
                .clients
                .iter()
                .find(|(_, client)| !client.closed.load(Ordering::SeqCst))
                .map(|(id, _)| *id);
        }
    }
    pub async fn chunk(&mut self, bytes: &[u8], stop: &mut watch::Receiver<Shutdown>) {
        self.history.extend_from_slice(bytes);
        if self.history.len() > 512 * 1024 {
            self.history.drain(..self.history.len() - 512 * 1024);
        }
        self.broadcast(output(bytes, false), stop).await;
    }
    pub async fn resize(
        &mut self,
        rows: u16,
        stop: &mut watch::Receiver<Shutdown>,
    ) -> Result<(), Error> {
        let Some(generation) = &mut self.generation else {
            return Ok(());
        };
        if !generation.alive()? || rows == self.rows {
            return Ok(());
        }
        self.rows = rows;
        openpilot_process_supervision::pty::resize(generation.master.get_ref(), self.rows, 100)?;
        generation.signal(rustix::process::Signal::WINCH)?;
        self.broadcast(
            Value::object([
                ("type", Value::text("pty_resize")),
                ("session", Value::text("login-shell")),
                ("rows", Value::integer(self.rows)),
                ("cols", Value::integer(100)),
            ]),
            stop,
        )
        .await;
        Ok(())
    }
    async fn broadcast(&mut self, payload: Value, stop: &mut watch::Receiver<Shutdown>) {
        let mut stale = Vec::new();
        for (id, client) in &self.clients {
            if client.closed.load(Ordering::SeqCst)
                || send(client, payload.clone(), stop).await.is_err()
            {
                stale.push(*id);
            }
        }
        for id in stale {
            self.clients.remove(&id);
            if self.primary == Some(id) {
                self.primary = None;
            }
        }
    }
    pub async fn finish(&mut self, code: Value, stop: &mut watch::Receiver<Shutdown>) {
        self.generation.take();
        let clients = std::mem::take(&mut self.clients);
        self.primary = None;
        for (_, client) in clients {
            if !client.closed.load(Ordering::SeqCst) {
                let payload = Value::object([
                    ("type", Value::text("pty_exit")),
                    ("session", Value::text("login-shell")),
                    ("exit_code", code.clone()),
                ]);
                if let Err(error) = send(&client, payload, stop).await {
                    eprintln!("PTY exit send: {error}");
                }
                client.finished.notify_one();
            }
        }
    }
}
