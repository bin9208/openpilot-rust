use crate::{
    services,
    state::{self, Options, State},
};
use openpilot_msgq::{MultiSubscriber, Publisher, Subscription};
use std::{
    collections::HashMap,
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    State(#[from] state::Error),
    #[error(transparent)]
    Transport(#[from] openpilot_msgq::Error),
}

pub struct SubMaster {
    pub state: State,
    subscriptions: MultiSubscriber,
}

impl SubMaster {
    pub fn for_runtime(names: &[&str], mut options: Options) -> Result<Self, Error> {
        let value = match std::env::var("SIMULATION") {
            Ok(value) => value,
            Err(std::env::VarError::NotPresent) => "0".to_owned(),
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(state::Error::Configuration("invalid SIMULATION value").into())
            }
        };
        options.simulation = value
            .trim()
            .parse::<i64>()
            .map_err(|_| state::Error::Configuration("SIMULATION must be an integer"))?
            != 0;
        Self::open(names, options, false)
    }

    pub fn isolated(names: &[&str], options: Options) -> Result<Self, Error> {
        Self::open(names, options, true)
    }

    fn open(names: &[&str], options: Options, isolated: bool) -> Result<Self, Error> {
        let state = State::new(names, options)?;
        let specifications: Vec<_> = state
            .topics()
            .iter()
            .map(|topic| Subscription {
                endpoint: topic.service.name,
                capacity: topic.service.queue_size,
                polled: topic.polled,
            })
            .collect();
        let subscriptions = if isolated {
            MultiSubscriber::new(&specifications)?
        } else {
            MultiSubscriber::for_runtime(&specifications)?
        };
        Ok(Self {
            state,
            subscriptions,
        })
    }

    pub fn update(&mut self, timeout: Duration) -> Result<(), Error> {
        let messages = self.subscriptions.receive(timeout)?;
        let messages: Vec<_> = messages.into_iter().map(|message| message.bytes).collect();
        Ok(self.state.update_with_clock(&messages, || {
            let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
            now.tv_sec as f64 + now.tv_nsec as f64 / 1e9
        })?)
    }
}

pub struct PubMaster {
    publishers: HashMap<&'static str, Publisher>,
}

impl PubMaster {
    pub fn for_runtime(names: &[&str]) -> Result<Self, Error> {
        Self::open(names, false)
    }
    pub fn isolated(names: &[&str]) -> Result<Self, Error> {
        Self::open(names, true)
    }

    fn open(names: &[&str], isolated: bool) -> Result<Self, Error> {
        let mut publishers = HashMap::with_capacity(names.len());
        for name in names {
            let service = services::lookup(name)
                .ok_or_else(|| state::Error::UnknownService((*name).to_owned()))?;
            if publishers.contains_key(service.name) {
                return Err(state::Error::Configuration("duplicate publisher").into());
            }
            let publisher = if isolated {
                Publisher::with_capacity(name, service.queue_size)?
            } else {
                Publisher::for_runtime(name, service.queue_size)?
            };
            publishers.insert(service.name, publisher);
        }
        Ok(Self { publishers })
    }

    pub fn send(&mut self, name: &str, bytes: &[u8]) -> Result<(), Error> {
        Ok(self.publisher(name)?.send(bytes)?)
    }

    pub fn all_readers_updated(&mut self, name: &str) -> Result<bool, Error> {
        Ok(self.publisher(name)?.readers_caught_up())
    }

    pub fn wait_for_readers(
        &mut self,
        name: &str,
        timeout: Duration,
        interval: Duration,
    ) -> Result<bool, Error> {
        let started = Instant::now();
        while !self.all_readers_updated(name)? {
            if started.elapsed() > timeout {
                return Ok(false);
            }
            thread::sleep(interval);
        }
        Ok(true)
    }

    fn publisher(&mut self, name: &str) -> Result<&mut Publisher, Error> {
        self.publishers
            .get_mut(name)
            .ok_or_else(|| state::Error::UnknownService(name.to_owned()).into())
    }
}
