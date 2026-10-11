use super::{
    context::{Audience, Context, Event},
    metadata,
    model::{Session, SessionData},
    policy::StartOptions,
    tunnel, webhook,
};
use crate::{web_sound::Shutdown, Error, Value};
use std::rc::Rc;
use tokio::sync::watch;

impl Context {
    pub async fn start_session(&self, options: StartOptions) -> Result<Value, Error> {
        let gate = self.gate.lock().await;
        if *self.stop.borrow() != Shutdown::Running {
            return Err(Error::Source("support terminal unavailable".into()));
        }
        if let Some(session) = self.current() {
            let reusable = {
                let session = session.borrow();
                matches!(session.state, "starting" | "sharing") && !session.expired()
            };
            if reusable {
                return Ok(session.borrow().snapshot());
            }
            self.stop_locked("restart").await?;
        }
        let session = SessionData::new(options)?;
        self.current.borrow_mut().replace(Rc::clone(&session));
        drop(gate);
        match self.prepare(&session).await {
            Ok(()) => Ok(self.snapshot()),
            Err(error) => {
                if self.is_current(&session) {
                    {
                        let mut session = session.borrow_mut();
                        session.state = "error";
                        session.error = error.to_string();
                        session.detail = "Start failed".into();
                    }
                    self.cleanup(Rc::clone(&session)).await?;
                    self.broadcast(
                        Audience::Owners,
                        Value::object([
                            ("type", Value::text("error")),
                            ("message", Value::text(&error.to_string())),
                        ]),
                    )
                    .await;
                    Ok(session.borrow().snapshot())
                } else {
                    self.cleanup(session).await?;
                    Ok(self.snapshot())
                }
            }
        }
    }
    async fn stale(&self, session: &Session) -> Result<bool, Error> {
        if self.is_current(session) {
            Ok(false)
        } else {
            self.cleanup(Rc::clone(session)).await?;
            Ok(true)
        }
    }
    async fn prepare(&self, session: &Session) -> Result<(), Error> {
        self.detail(session, "Preparing shared terminal session")
            .await;
        self.config.terminal.ensure().await?;
        if self.stale(session).await? {
            return Ok(());
        }
        self.detail(session, "Starting support page").await;
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
        let origin = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
        let (stop, stopped) = watch::channel(false);
        {
            let mut session = session.borrow_mut();
            session.origin = origin.clone();
            session.local_url = format!("{origin}/support/terminal/{}", session.id);
            session.local_stop = Some(stop);
        }
        self.event(Event::Local(listener, Rc::clone(session), stopped))?;
        if self.stale(session).await? {
            return Ok(());
        }
        self.detail(
            session,
            if self.binary.path().is_some() {
                "Starting secure tunnel"
            } else if self.binary.automatic() && !self.binary.download.is_empty() {
                "Downloading cloudflared"
            } else {
                "cloudflared unavailable"
            },
        )
        .await;
        let tunnel =
            tunnel::Tunnel::start(&self.config, &self.binary, &self.download, &origin).await?;
        session.borrow_mut().tunnel = Some(tunnel);
        if self.stale(session).await? {
            return Ok(());
        }
        {
            let mut session = session.borrow_mut();
            let base = session
                .tunnel
                .as_ref()
                .map_or("", |tunnel| tunnel.url.as_str())
                .trim_end_matches('/');
            session.url = if base.contains("/support/terminal/") {
                base.into()
            } else {
                format!("{base}/support/terminal/{}", session.id)
            };
            session.state = "sharing";
        }
        self.detail(session, "Secure tunnel ready").await;
        if self.config.public_delay > 0. {
            tokio::time::sleep(
                std::time::Duration::try_from_secs_f64(self.config.public_delay)
                    .map_err(|error| Error::Source(error.to_string()))?,
            )
            .await;
        }
        if self.stale(session).await? {
            return Ok(());
        }
        self.event(Event::Expiry(Rc::clone(session)))?;
        self.detail(session, "Sending Carrot server notification")
            .await;
        let metadata = metadata::collect(&self.config, &self.runner).await?;
        if self.stale(session).await? {
            return Ok(());
        }
        let result = webhook::send(&self.http, session, &metadata).await;
        session.borrow_mut().discord = result;
        if self.stale(session).await? {
            return Ok(());
        }
        self.detail(session, "Ready").await;
        self.broadcast(Audience::Owners, super::context::status(session))
            .await;
        Ok(())
    }
    pub async fn expire_session(&self, session: Session) -> Result<(), Error> {
        let mut cancelled = session.borrow().expiry_stop.subscribe();
        loop {
            if *cancelled.borrow() {
                return Ok(());
            }
            if session.borrow().expired() {
                break;
            }
            self.broadcast(Audience::Owners, super::context::status(&session))
                .await;
            tokio::select! { _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {}, _ = cancelled.changed() => return Ok(()) }
        }
        if self.is_current(&session) {
            self.stop_session("expired").await?;
        }
        Ok(())
    }
}
