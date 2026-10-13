pub(crate) mod body;
pub mod environment;
mod http;
mod online;
use crate::{params::Backend, Error, Value};
pub use environment::Environment;
pub use http::handle;
pub use online::Online;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

#[derive(Debug, PartialEq, Eq)]
pub enum LoopExit {
    Returned,
    Cancelled,
}

pub struct Service {
    last: Mutex<Value>,
    online: Online,
    environment: Environment,
}

impl Service {
    pub fn new() -> Arc<Self> {
        Self::with_dependencies(Online::default(), Environment::default())
    }

    pub fn with_dependencies(online: Online, environment: Environment) -> Arc<Self> {
        Arc::new(Self {
            last: Mutex::new(Value::object([
                ("ok", Value::Null),
                ("msg", Value::text("not yet")),
                ("ts", Value::integer(0)),
            ])),
            online,
            environment,
        })
    }

    pub fn snapshot(&self) -> Result<Value, Error> {
        self.last
            .lock()
            .map(|value| value.clone())
            .map_err(|_| Error::Source("heartbeat state lock poisoned".into()))
    }

    fn store(&self, value: Value) -> Result<(), Error> {
        *self
            .last
            .lock()
            .map_err(|_| Error::Source("heartbeat state lock poisoned".into()))? = value;
        Ok(())
    }

    fn payload(&self, params: &Backend) -> Result<Value, Error> {
        let local_ip = (self.environment.local_ip)();
        let version = params.typed_value("Version", false).unwrap_or(Value::Null);
        let github = params
            .typed_value("GithubUsername", false)
            .unwrap_or(Value::Null);
        let onroad = params.get("IsOnroad", &Value::Bool(false)).truth();
        let timestamp = Value::Float((self.environment.time)()).int()?;
        Ok(Value::object([
            ("github_id", github),
            ("token", Value::text("12345678")),
            ("local_ip", Value::text(&local_ip)),
            ("port", Value::integer(7000)),
            ("version", version),
            ("is_onroad", Value::Bool(onroad)),
            ("ts", Value::Integer(timestamp)),
        ]))
    }

    pub fn register(&self, params: &Backend) -> (bool, String) {
        match self
            .payload(params)
            .and_then(|payload| self.online.post(&payload))
        {
            Ok(result) => result,
            Err(error) => (false, format!("Exception: {error}")),
        }
    }

    fn complete(&self, ok: bool, message: &str) -> Result<(), Error> {
        let message: String = message.chars().take(800).collect();
        let timestamp = (self.environment.time)();
        let local_ip = (self.environment.local_ip)();
        self.store(Value::object([
            ("ok", Value::Bool(ok)),
            ("msg", Value::text(&message)),
            ("ts", Value::Float(timestamp)),
            ("local_ip", Value::text(&local_ip)),
        ]))
    }

    pub async fn run_loop(
        self: Arc<Self>,
        params: Backend,
        mut stop: watch::Receiver<bool>,
    ) -> Result<LoopExit, Error> {
        if !params.has_params() {
            self.store(Value::object([
                ("ok", Value::Bool(false)),
                ("msg", Value::text("Params not available")),
            ]))?;
            return Ok(LoopExit::Returned);
        }
        let params = Arc::new(params);
        loop {
            if *stop.borrow() {
                return Ok(LoopExit::Returned);
            }
            let context = self.clone();
            let params = params.clone();
            let request = tokio::task::spawn_blocking(move || context.register(&params));
            let result = tokio::select! {
                biased;
                _ = stop.changed() => return Ok(LoopExit::Returned),
                result = request => result,
            };
            match result {
                Ok((ok, message)) => self.complete(ok, &message)?,
                Err(error) => self.store(Value::object([
                    ("ok", Value::Bool(false)),
                    ("msg", Value::text(&format!("Exception: {error}"))),
                    ("ts", Value::Float((self.environment.time)())),
                ]))?,
            }
            tokio::select! {
                biased;
                _ = stop.changed() => return Ok(LoopExit::Cancelled),
                _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {},
            }
        }
    }
}
