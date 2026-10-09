mod guard;
mod presets;
mod routes;
mod state;

use crate::{config::Config, params::Backend, settings::SettingsCache, Error, Value};
pub use routes::{handle, Route};
use std::{
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

enum Clock {
    System,
    Fixed(i64),
}

pub struct Intro {
    config: Config,
    settings: Mutex<SettingsCache>,
    state_lock: Mutex<()>,
    clock: Clock,
}

impl Intro {
    pub fn new(config: Config) -> Arc<Self> {
        Self::create(config, Clock::System)
    }

    pub fn with_timestamp(config: Config, timestamp: i64) -> Arc<Self> {
        Self::create(config, Clock::Fixed(timestamp))
    }

    fn create(config: Config, clock: Clock) -> Arc<Self> {
        Arc::new(Self {
            settings: Mutex::new(SettingsCache::new(config.settings.clone())),
            config,
            state_lock: Mutex::new(()),
            clock,
        })
    }

    fn timestamp(&self) -> Value {
        match self.clock {
            Clock::Fixed(seconds) => Value::integer(seconds),
            Clock::System => match SystemTime::now().duration_since(UNIX_EPOCH) {
                Ok(time) => Value::integer(time.as_secs()),
                Err(error) => Value::Integer(-num_bigint::BigInt::from(error.duration().as_secs())),
            },
        }
    }

    fn definitions(&self, params: &Backend) -> Result<Value, Error> {
        let mut cache = self
            .settings
            .lock()
            .map_err(|_| Error::Source("intro settings lock poisoned".into()))?;
        Ok(cache.load(params.maximum_gap_levels())?.by_name)
    }

    pub fn read_state(&self) -> Result<Value, Error> {
        let _guard = self
            .state_lock
            .lock()
            .map_err(|_| Error::Source("intro state lock poisoned".into()))?;
        self.read_locked()
    }

    pub fn mark_completed(&self, reason: &Value) -> Result<Value, Error> {
        let _guard = self
            .state_lock
            .lock()
            .map_err(|_| Error::Source("intro state lock poisoned".into()))?;
        self.mark_locked(reason)
    }

    pub fn bootstrap(&self, params: &Backend) -> Result<Value, Error> {
        let _guard = self
            .state_lock
            .lock()
            .map_err(|_| Error::Source("intro state lock poisoned".into()))?;
        self.bootstrap_locked(params)
    }

    pub fn state_payload(&self, params: &Backend) -> Result<Value, Error> {
        let _guard = self
            .state_lock
            .lock()
            .map_err(|_| Error::Source("intro state lock poisoned".into()))?;
        let bootstrap = self.bootstrap_locked(params)?;
        Ok(Value::object([
            ("ok", Value::Bool(true)),
            ("shouldShow", bootstrap.get("shouldShow").clone()),
            ("reason", bootstrap.get("reason").clone()),
            ("state", self.read_locked()?),
        ]))
    }
}
