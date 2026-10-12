mod commands;
mod guest;
mod model;
mod sockets;
pub mod transport;

use crate::{config::Config, terminal::Terminal, Error, Value};
use model::{event, idle, Session, State};
use num_traits::ToPrimitive;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::Duration,
};

pub struct Support {
    config: Config,
    terminal: Arc<Terminal>,
    session: Mutex<Option<Arc<Session>>>,
    workers: Mutex<Vec<JoinHandle<()>>>,
    next: AtomicU64,
    shutdown: AtomicBool,
    ttl: i64,
    timeout: i64,
}
impl Support {
    pub fn new(config: Config, terminal: Arc<Terminal>) -> Result<Arc<Self>, Error> {
        let setting = |name, default: &str| -> Result<i64, Error> {
            Value::text(&std::env::var(name).unwrap_or_else(|_| default.into()))
                .int()?
                .to_i64()
                .ok_or_else(|| Error::Source("support timing value out of range".into()))
        };
        Ok(Arc::new(Self {
            config,
            terminal,
            session: Mutex::new(None),
            workers: Mutex::new(Vec::new()),
            next: AtomicU64::new(1),
            shutdown: AtomicBool::new(false),
            ttl: setting("CARROT_SUPPORT_TTL_SECONDS", "1800")?,
            timeout: setting("CARROT_SUPPORT_COMMAND_TIMEOUT_SECONDS", "30")?,
        }))
    }
    fn current(&self) -> Result<Option<Arc<Session>>, Error> {
        Ok(self
            .session
            .lock()
            .map_err(|_| Error::Source("support manager poisoned".into()))?
            .clone())
    }
    pub fn snapshot(&self) -> Result<Value, Error> {
        self.current()?
            .map_or_else(|| Ok(idle()), |session| session.snapshot())
    }
    fn spawn(&self, run: impl FnOnce() + Send + 'static) -> Result<(), Error> {
        let mut workers = self
            .workers
            .lock()
            .map_err(|_| Error::Source("support worker registry poisoned".into()))?;
        if self.shutdown.load(Ordering::SeqCst) {
            return Err(Error::Source("support manager stopped".into()));
        }
        let worker = std::thread::Builder::new()
            .name("recovery-support".into())
            .spawn(run)?;
        while let Some(index) = workers.iter().position(JoinHandle::is_finished) {
            if workers.swap_remove(index).join().is_err() {
                eprintln!("[recovery] support worker panicked");
            }
        }
        workers.push(worker);
        Ok(())
    }
    pub fn start(self: &Arc<Self>, payload: &Value) -> Result<Value, Error> {
        if self.shutdown.load(Ordering::SeqCst) {
            return Err(Error::Source("support manager stopped".into()));
        }
        let session = {
            let mut current = self
                .session
                .lock()
                .map_err(|_| Error::Source("support manager poisoned".into()))?;
            if let Some(session) = current.as_ref() {
                if session.data()?.state.active() && !session.expired() {
                    return session.snapshot();
                }
                self.cleanup(session, "restart")?;
            }
            let session = Session::new(payload, self.ttl, self.timeout)?;
            *current = Some(Arc::clone(&session));
            session
        };
        let operation = || -> Result<(), Error> {
            self.terminal.ensure()?;
            session.detail("Starting support page")?;
            let local = guest::start(Arc::clone(self), Arc::clone(&session))?;
            session.detail("Starting secure tunnel")?;
            let tunnel =
                transport::Tunnel::start(&self.config, &local, Arc::clone(&session.stopped))?;
            if session.stopped.load(Ordering::SeqCst) {
                return Err(Error::Source("support session stopped".into()));
            }
            {
                let mut data = session.data()?;
                if session.stopped.load(Ordering::SeqCst) {
                    return Err(Error::Source("support session stopped".into()));
                }
                let base = tunnel.url.trim_end_matches('/');
                data.url = if base.contains("/support/terminal/") {
                    base.into()
                } else {
                    format!("{base}/support/terminal/{}", session.id)
                };
                data.tunnel = Some(tunnel);
                data.state = State::Sharing;
            }
            session.detail("Secure tunnel ready")?;
            let service = Arc::clone(self);
            let active = Arc::clone(&session);
            self.spawn(move || {
                if let Err(error) = service.monitor(&active) {
                    eprintln!("[recovery] support expiry: {error}");
                }
            })?;
            let service = Arc::clone(self);
            let active = Arc::clone(&session);
            self.spawn(move || {
                if let Err(error) = service.notify(&active) {
                    eprintln!("[recovery] support notification: {error}");
                }
            })?;
            session.detail("Ready")
        };
        if let Err(error) = operation() {
            if !session.stopped.load(Ordering::SeqCst) {
                {
                    let mut data = session.data()?;
                    data.state = State::Error;
                    data.error = error.to_string();
                    data.detail = "Start failed".into();
                }
                self.cleanup(&session, "error")?;
                session.owners(&Value::object([
                    ("type", Value::text("error")),
                    ("message", Value::text(&error.to_string())),
                ]))?;
                return session.snapshot();
            }
            return self.snapshot();
        }
        session.snapshot()
    }
    fn notify(&self, session: &Session) -> Result<(), Error> {
        use chrono::{Local, TimeZone};
        let timestamp = Local
            .timestamp_opt(session.created.to_i64().unwrap_or(0), 0)
            .single()
            .ok_or_else(|| Error::Source("support timestamp out of range".into()))?;
        let payload = Value::object([
            ("sessionId", Value::text(&session.id)),
            (
                "createdAt",
                Value::text(&timestamp.format("%Y-%m-%d %H:%M:%S").to_string()),
            ),
            ("url", Value::text(&session.data()?.url)),
            ("pin", Value::text(&session.pin)),
            ("ttl_minutes", Value::integer((session.ttl / 60).max(1))),
            ("permissionMode", Value::text(session.permission.name())),
            ("commandTimeoutSeconds", Value::integer(session.timeout)),
            ("meta", transport::metadata(&self.config)?),
            ("note", Value::text(&session.note)),
        ]);
        let result = transport::notify(&self.config, &payload);
        session.data()?.discord = result;
        session.owners(&event("session_status", session.snapshot()?))
    }
    pub fn stop(&self, reason: &str) -> Result<Value, Error> {
        let active = self
            .session
            .lock()
            .map_err(|_| Error::Source("support manager poisoned".into()))?
            .take();
        if let Some(active) = active {
            self.cleanup(&active, reason)?;
        }
        Ok(idle())
    }
    fn cleanup(&self, session: &Session, reason: &str) -> Result<(), Error> {
        session.stopped.store(true, Ordering::SeqCst);
        session.wake.notify_all();
        if reason != "error" {
            session.data()?.state = if reason == "expired" {
                State::Expired
            } else {
                State::Stopped
            };
            session.all(&Value::object([
                ("type", Value::text("session_closed")),
                ("reason", Value::text(reason)),
            ]))?;
        }
        let (owners, guests, tunnel) = {
            let mut data = session.data()?;
            data.controller = None;
            (
                std::mem::take(&mut data.owners),
                std::mem::take(&mut data.guests),
                data.tunnel.take(),
            )
        };
        for (_, sender) in owners {
            sender.close();
        }
        for (_, guest) in guests {
            if let Some(id) = guest.terminal {
                if let Err(error) = self.terminal.detach(id) {
                    eprintln!("[recovery] support detach: {error}");
                }
            }
            guest.sender.close();
        }
        crate::http::registry::close(&session.sockets)?;
        drop(tunnel);
        Ok(())
    }
    fn monitor(&self, session: &Session) -> Result<(), Error> {
        let mut next_status = model::now();
        while !session.stopped.load(Ordering::SeqCst) {
            if session.expired() {
                let mut current = self
                    .session
                    .lock()
                    .map_err(|_| Error::Source("support manager poisoned".into()))?;
                if current
                    .as_ref()
                    .is_some_and(|active| active.id == session.id)
                {
                    current.take();
                    self.cleanup(session, "expired")?;
                }
                break;
            }
            let now = model::now();
            let expired = {
                let mut data = session.data()?;
                let mut expired = Vec::new();
                data.pending.retain(|command| {
                    if now - command.created >= session.timeout.to_f64().unwrap_or(30.) {
                        expired.push(command.id.clone());
                        false
                    } else {
                        true
                    }
                });
                expired
            };
            for id in expired {
                session.all(&Value::object([
                    ("type", Value::text("command_expired")),
                    ("id", Value::text(&id)),
                ]))?;
            }
            if now >= next_status {
                session.owners(&event("session_status", session.snapshot()?))?;
                next_status = now + 1.;
            }
            let next = session
                .data()?
                .pending
                .iter()
                .map(|command| command.created + session.timeout.to_f64().unwrap_or(30.))
                .fold(next_status.min(session.expires), f64::min);
            let wait = session
                .wait
                .lock()
                .map_err(|_| Error::Source("support timer poisoned".into()))?;
            let _wait = session
                .wake
                .wait_timeout(
                    wait,
                    Duration::from_secs_f64((next - model::now()).max(0.001)),
                )
                .map_err(|_| Error::Source("support timer poisoned".into()))?;
        }
        Ok(())
    }
    pub fn shutdown(&self) -> Result<(), Error> {
        self.shutdown.store(true, Ordering::SeqCst);
        let stopped = self.stop("shutdown");
        loop {
            let workers = std::mem::take(
                &mut *self
                    .workers
                    .lock()
                    .map_err(|_| Error::Source("support worker registry poisoned".into()))?,
            );
            if workers.is_empty() {
                break;
            }
            for worker in workers {
                if worker.join().is_err() {
                    eprintln!("[recovery] support worker panicked during shutdown");
                }
            }
        }
        stopped.map(|_| ())
    }
}
