use super::{
    pty::{exit_code, Generation},
    Command, Reply,
};
use crate::{config::Config, websocket::Sender, Error, Value};
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{
    collections::BTreeMap,
    sync::{mpsc, Arc},
    time::Duration,
};

const SESSION: &str = "recovery-shell";
const HISTORY_LIMIT: usize = 512 * 1024;
struct State {
    generation: Option<Generation>,
    clients: BTreeMap<u64, Arc<Sender>>,
    history: Vec<u8>,
    rows: u16,
    next: u64,
}
fn output(bytes: &[u8], replay: bool) -> Value {
    let mut fields = vec![
        (
            "type".chars().map(u32::from).collect(),
            Value::text("pty_output"),
        ),
        (
            "session".chars().map(u32::from).collect(),
            Value::text(SESSION),
        ),
        (
            "b64".chars().map(u32::from).collect(),
            Value::text(&STANDARD.encode(bytes)),
        ),
    ];
    if replay {
        fields.push(("replay".chars().map(u32::from).collect(), Value::Bool(true)));
    }
    Value::Object(fields)
}
impl State {
    fn new() -> Self {
        Self {
            generation: None,
            clients: BTreeMap::new(),
            history: Vec::new(),
            rows: 30,
            next: 0,
        }
    }
    fn finish(&mut self, code: Value) {
        self.generation.take();
        let payload = Value::object([
            ("type", Value::text("pty_exit")),
            ("session", Value::text(SESSION)),
            ("exit_code", code),
        ]);
        for client in std::mem::take(&mut self.clients).into_values() {
            let _sent = client.send_json(&payload);
            client.close();
        }
    }
    fn ensure(&mut self, config: &Config) -> Result<bool, Error> {
        if let Some(generation) = &self.generation {
            match generation.observe()? {
                None => return Ok(false),
                Some(status) => self.finish(Value::integer(exit_code(status))),
            }
        }
        self.history.clear();
        self.rows = 30;
        self.generation = Some(Generation::start(config)?);
        Ok(true)
    }
    fn attach(&mut self, config: &Config, client: Arc<Sender>, reset: bool) -> Result<u64, Error> {
        if reset {
            self.finish(Value::Null);
            self.history.clear();
        }
        let created = self.ensure(config)?;
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| Error::Source("terminal client ID exhausted".into()))?;
        let meta = Value::object([
            ("type", Value::text("meta")),
            ("mode", Value::text("pty")),
            ("session", Value::text(SESSION)),
            ("created", Value::Bool(created)),
            ("user", Value::text("comma")),
            ("rows", Value::integer(self.rows)),
            ("cols", Value::integer(100)),
        ]);
        let sent = client.send_json(&meta)
            && (self.history.is_empty() || client.send_json(&output(&self.history, true)));
        if sent {
            self.clients.insert(self.next, client);
        }
        Ok(self.next)
    }
    fn broadcast(&mut self, payload: &Value) {
        self.clients.retain(|_, client| client.send_json(payload));
    }
    fn write(&self, bytes: &[u8]) -> Result<(), Error> {
        match &self.generation {
            Some(generation) if generation.observe()?.is_none() => generation.write(bytes),
            Some(_) | None => Err(Error::Source("terminal session is not running".into())),
        }
    }
    fn resize(&mut self, rows: i64) -> Result<(), Error> {
        let rows = u16::try_from(if rows == 0 { 30 } else { rows }.clamp(8, 200))
            .map_err(|_| Error::Source("invalid PTY rows".into()))?;
        let Some(generation) = &self.generation else {
            return Ok(());
        };
        if generation.observe()?.is_some() || rows == self.rows {
            return Ok(());
        }
        openpilot_process_supervision::pty::resize(&generation.master, rows, 100)?;
        self.rows = rows;
        generation.signal(rustix::process::Signal::WINCH)?;
        self.broadcast(&Value::object([
            ("type", Value::text("pty_resize")),
            ("session", Value::text(SESSION)),
            ("rows", Value::integer(rows)),
            ("cols", Value::integer(100)),
        ]));
        Ok(())
    }
    fn read(&mut self) -> Result<(), Error> {
        let Some(generation) = &self.generation else {
            return Ok(());
        };
        if let Some(status) = generation.observe()? {
            self.finish(Value::integer(exit_code(status)));
            return Ok(());
        }
        if !generation.ready()? {
            return Ok(());
        }
        let mut bytes = [0; 4096];
        match rustix::io::read(&generation.master, &mut bytes) {
            Ok(0) | Err(rustix::io::Errno::IO) => {
                let code = generation
                    .observe_eof()?
                    .map_or(Value::Null, |status| Value::integer(exit_code(status)));
                self.finish(code);
            }
            Ok(count) => {
                let bytes = &bytes[..count];
                self.history.extend_from_slice(bytes);
                if self.history.len() > HISTORY_LIMIT {
                    self.history.drain(..self.history.len() - HISTORY_LIMIT);
                }
                self.broadcast(&output(bytes, false));
            }
            Err(rustix::io::Errno::AGAIN | rustix::io::Errno::INTR) => {}
            Err(error) => return Err(std::io::Error::from(error).into()),
        }
        Ok(())
    }
}
fn reply<T>(channel: Reply<T>, result: Result<T, Error>) {
    if channel.send(result).is_err() {
        eprintln!("[recovery] terminal request caller closed");
    }
}
pub(super) fn run(config: Config, commands: mpsc::Receiver<Command>) {
    let mut state = State::new();
    loop {
        let command = if state.generation.is_some() {
            commands.try_recv().map_err(|error| match error {
                mpsc::TryRecvError::Empty => mpsc::RecvTimeoutError::Timeout,
                mpsc::TryRecvError::Disconnected => mpsc::RecvTimeoutError::Disconnected,
            })
        } else {
            commands.recv_timeout(Duration::from_millis(10))
        };
        match command {
            Ok(Command::Ensure(channel)) => {
                let result = state.ensure(&config);
                reply(channel, result);
            }
            Ok(Command::Attach {
                client,
                reset,
                reply: channel,
            }) => {
                let result = state.attach(&config, client, reset);
                reply(channel, result);
            }
            Ok(Command::Detach { id, reply: channel }) => {
                state.clients.remove(&id);
                reply(channel, Ok(()));
            }
            Ok(Command::Write {
                bytes,
                reply: channel,
            }) => reply(channel, state.write(&bytes)),
            Ok(Command::Resize {
                rows,
                reply: channel,
            }) => {
                let result = state.resize(rows);
                reply(channel, result);
            }
            Ok(Command::Clear(channel)) => {
                state.history.clear();
                reply(channel, Ok(()));
            }
            Ok(Command::Stop(channel)) => {
                state.finish(Value::Null);
                state.history.clear();
                reply(channel, Ok(()));
                break;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if let Err(error) = state.read() {
            eprintln!("[recovery] PTY read: {error}");
            state.finish(Value::Null);
        }
    }
    state.finish(Value::Null);
}
