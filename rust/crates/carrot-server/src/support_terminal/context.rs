use super::{
    admission::Counter,
    model::{self, Client, Session},
    Config,
};
use crate::{tools::runner::Runner, web_sound::Shutdown, Error, Value};
use futures_util::SinkExt;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{atomic::Ordering, Arc},
};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;

pub(super) enum Event {
    Cleanup(Session),
    Expiry(Session),
    CommandExpiry(Session, String),
    Local(tokio::net::TcpListener, Session, watch::Receiver<bool>),
    Socket(
        hyper::upgrade::OnUpgrade,
        super::session::Mode,
        super::admission::Admission,
    ),
}
pub(super) struct Queued {
    pub event: Event,
    pub admission: super::admission::Admission,
}
pub(super) struct Context {
    pub gate: tokio::sync::Mutex<()>,
    pub current: RefCell<Option<Session>>,
    pub config: Config,
    pub binary: super::tunnel_binary::Config,
    pub http: reqwest::Client,
    pub download: reqwest::Client,
    pub runner: Runner,
    pub stop: watch::Receiver<Shutdown>,
    pub events: mpsc::UnboundedSender<Queued>,
    pub count: Arc<Counter>,
}
#[derive(Clone, Copy)]
pub(super) enum Audience {
    Owners,
    Guests,
    All,
}
impl Context {
    pub fn current(&self) -> Option<Session> {
        self.current.borrow().clone()
    }
    pub fn snapshot(&self) -> Value {
        self.current()
            .map_or_else(model::idle, |session| session.borrow().snapshot())
    }
    pub fn is_current(&self, session: &Session) -> bool {
        self.current
            .borrow()
            .as_ref()
            .is_some_and(|current| Rc::ptr_eq(current, session))
    }
    pub fn event(&self, event: Event) -> Result<(), Error> {
        self.events
            .send(Queued {
                event,
                admission: self.count.owned(),
            })
            .map_err(|_| Error::Source("support owner unavailable".into()))
    }
    pub async fn send(&self, client: &Client, value: Value) -> Result<(), Error> {
        if client.closed.load(Ordering::SeqCst) {
            return Ok(());
        }
        let operation = async {
            client
                .sink
                .lock()
                .await
                .send(Message::text(value.encode()?))
                .await
                .map_err(|error| Error::Source(error.to_string()))
        };
        let mut stopped = self.stop.clone();
        tokio::pin!(operation);
        loop {
            if *stopped.borrow() == Shutdown::Force {
                return Err(Error::Source("support owner stopping".into()));
            }
            tokio::select! { result = &mut operation => return result, changed = stopped.changed() => if changed.is_err() { return Err(Error::Source("support owner stopping".into())); } }
        }
    }
    pub async fn broadcast(&self, audience: Audience, payload: Value) {
        let Some(session) = self.current() else {
            return;
        };
        let clients: Vec<_> = {
            let session = session.borrow();
            match audience {
                Audience::Owners => session
                    .owners
                    .iter()
                    .map(|(id, client)| (*id, client.clone(), true))
                    .collect(),
                Audience::Guests => session
                    .guests
                    .iter()
                    .map(|(id, client)| (*id, client.clone(), false))
                    .collect(),
                Audience::All => session
                    .owners
                    .iter()
                    .map(|(id, client)| (*id, client.clone(), true))
                    .chain(
                        session
                            .guests
                            .iter()
                            .map(|(id, client)| (*id, client.clone(), false)),
                    )
                    .collect(),
            }
        };
        for (id, client, owner) in clients {
            if client.closed.load(Ordering::SeqCst)
                || self.send(&client, payload.clone()).await.is_err()
            {
                if owner {
                    session.borrow_mut().owners.remove(&id);
                } else {
                    session.borrow_mut().guests.remove(&id);
                }
            }
        }
    }
    pub async fn detail(&self, session: &Session, detail: &str) {
        session.borrow_mut().detail = detail.into();
        if self.is_current(session) {
            self.broadcast(Audience::Owners, status(session)).await;
        }
    }
    pub async fn stop_session(&self, reason: &str) -> Result<Value, Error> {
        let _gate = self.gate.lock().await;
        self.stop_locked(reason).await
    }
    pub async fn stop_locked(&self, reason: &str) -> Result<Value, Error> {
        if let Some(session) = self.current() {
            session.borrow_mut().state = if reason == "expired" {
                "expired"
            } else {
                "stopped"
            };
            self.broadcast(
                Audience::All,
                Value::object([
                    ("type", Value::text("session_closed")),
                    ("reason", Value::text(reason)),
                ]),
            )
            .await;
            self.current.borrow_mut().take();
            self.event(Event::Cleanup(session))?;
        }
        Ok(self.snapshot())
    }
    pub async fn cleanup(&self, session: Session) -> Result<(), Error> {
        session.borrow().expiry_stop.send_replace(true);
        let clients: Vec<_> = {
            let session = session.borrow();
            session
                .guests
                .values()
                .chain(session.owners.values())
                .cloned()
                .collect()
        };
        for client in clients {
            if let Some(id) = client.terminal_id {
                if let Err(error) = self.config.terminal.detach_guest(id).await {
                    eprintln!("Support guest detach: {error}");
                }
            }
            client.finished.notify_one();
        }
        {
            let mut session = session.borrow_mut();
            session.guests.clear();
            session.owners.clear();
            session.controller = None;
        }
        let tunnel = session.borrow_mut().tunnel.take();
        if let Some(mut tunnel) = tunnel {
            tunnel.stop().await?;
        }
        if let Some(stop) = session.borrow_mut().local_stop.take() {
            stop.send_replace(true);
        }
        Ok(())
    }
    pub async fn roles(&self, session: &Session) {
        let clients: Vec<_> = {
            let mut session = session.borrow_mut();
            if session.options.permission == super::policy::Permission::ApproveEach {
                session.controller = None;
            } else if !session.controller_present() {
                session.controller = session
                    .guests
                    .iter()
                    .find(|(_, client)| !client.closed.load(Ordering::SeqCst))
                    .map(|(id, _)| *id);
            }
            session
                .guests
                .iter()
                .map(|(id, client)| (session.grant(*id), client.clone()))
                .collect()
        };
        for (granted, client) in clients {
            if let Err(error) = self
                .send(
                    &client,
                    Value::object([
                        ("type", Value::text("control_role")),
                        ("granted", Value::Bool(granted)),
                    ]),
                )
                .await
            {
                eprintln!("Support control role: {error}");
            }
        }
    }
}
pub(super) fn status(session: &Session) -> Value {
    let mut value = Value::object([("type", Value::text("session_status"))]);
    if let (Value::Object(fields), Value::Object(status)) =
        (&mut value, session.borrow().snapshot())
    {
        fields.extend(status);
    }
    value
}
