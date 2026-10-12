use super::{protocol, tmux, Config};
use crate::{tools::runner::Runner, web_sound::socket::Sink, Error, Value};
use futures_util::SinkExt;
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone)]
pub(super) struct Context {
    pub config: Config,
    runner: Runner,
    session: String,
    sink: Sink,
    last: Arc<Mutex<Option<String>>>,
}
impl Context {
    pub fn new(config: Config, runner: Runner, session: String, sink: Sink) -> Self {
        Self {
            config,
            runner,
            session,
            sink,
            last: Arc::new(Mutex::new(None)),
        }
    }
    async fn send(&self, value: Value) -> Result<(), Error> {
        self.sink
            .lock()
            .await
            .send(Message::text(value.encode()?))
            .await
            .map_err(|error| Error::Source(error.to_string()))
    }
    async fn meta(&self, created: bool) -> Result<(), Error> {
        self.send(Value::object([
            ("type", Value::text("meta")),
            ("session", Value::text(&self.session)),
            ("created", Value::Bool(created)),
            ("user", Value::text("comma")),
        ]))
        .await
    }
    pub async fn initialize(&self) -> Result<(), Error> {
        self.meta(tmux::ensure(&self.config, &self.runner, &self.session).await?)
            .await?;
        Ok(())
    }
    pub async fn initial_screen(&self) -> Result<(), Error> {
        self.screen(true, 0.02).await
    }
    async fn screen(&self, force: bool, delay: f64) -> Result<(), Error> {
        if delay > 0. {
            tokio::time::sleep(Duration::from_secs_f64(delay)).await;
        }
        let current = tmux::capture(&self.config, &self.runner, &self.session).await?;
        let mut last = self.last.lock().await;
        if force || last.as_ref() != Some(&current) {
            self.send(Value::object([
                ("type", Value::text("screen")),
                ("session", Value::text(&self.session)),
                ("text", Value::text(&current)),
            ]))
            .await?;
            *last = Some(current);
        }
        Ok(())
    }
    pub async fn pump(&self) -> Result<(), Error> {
        loop {
            tokio::time::sleep(Duration::from_millis(180)).await;
            self.screen(false, 0.).await?;
        }
    }
    pub async fn error(&self, error: &Error) -> Result<(), Error> {
        self.send(Value::object([
            ("type", Value::text("error")),
            ("error", Value::text(&error.to_string())),
            ("session", Value::text(&self.session)),
        ]))
        .await
    }
    pub async fn input(&self, value: Value) -> Result<(), Error> {
        let result = if value.get("type").text_eq("input") {
            let line = protocol::translate(&self.config, &protocol::text(value.get("data"))?, true);
            tmux::line(&self.runner, &self.session, &line).await?;
            self.screen(true, 0.03).await
        } else if value.get("type").text_eq("control") {
            let action = value.get("action");
            if action.truth() && !matches!(action, Value::Text(_)) {
                return self
                    .error(&Error::Source(format!(
                        "'{}' object has no attribute 'strip'",
                        action.type_name()
                    )))
                    .await;
            }
            match protocol::text(action)?.trim() {
                "ctrl_c" => {
                    tmux::keys(&self.runner, &self.session, &["C-c".into()], false).await?;
                    self.screen(true, 0.03).await
                }
                "clear" => {
                    tmux::line(&self.runner, &self.session, "clear").await?;
                    tokio::time::sleep(Duration::from_millis(40)).await;
                    tmux::run(
                        &self.runner,
                        &[
                            "tmux".into(),
                            "clear-history".into(),
                            "-t".into(),
                            self.session.clone(),
                        ],
                        4.,
                        false,
                    )
                    .await?;
                    self.screen(true, 0.05).await
                }
                "refresh" => self.screen(true, 0.).await,
                "new_session" => {
                    self.meta(tmux::ensure(&self.config, &self.runner, &self.session).await?)
                        .await?;
                    self.screen(true, 0.08).await
                }
                _ => Ok(()),
            }
        } else {
            Ok(())
        };
        if let Err(error) = result {
            self.error(&error).await?;
        }
        Ok(())
    }
}
