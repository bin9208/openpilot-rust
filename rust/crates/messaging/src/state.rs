use crate::{
    frequency::{self, FrequencyTracker},
    services::{self, Service},
};
use capnp::{
    dynamic_value,
    message::{self, ReaderOptions},
    serialize::{self, OwnedSegments},
};
use openpilot_cereal::log_capnp::event;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    io::Cursor,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid subscription configuration: {0}")]
    Configuration(&'static str),
    #[error("unknown service: {0}")]
    UnknownService(String),
    #[error("invalid cereal event: {0}")]
    Cereal(#[from] capnp::Error),
    #[error("invalid schema service name: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    #[error(transparent)]
    Frequency(#[from] frequency::Error),
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Poll {
    #[default]
    All,
    One(String),
    Many(Vec<String>),
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Options {
    pub poll: Poll,
    pub frequency: Option<f64>,
    pub ignore_alive: Vec<String>,
    pub ignore_frequency: Vec<String>,
    pub ignore_valid: Vec<String>,
    pub simulation: bool,
}

#[derive(Serialize)]
pub struct Topic {
    pub service: &'static Service,
    pub seen: bool,
    pub updated: bool,
    pub receive_time: f64,
    pub receive_frame: i64,
    pub log_mono_time: u64,
    pub alive: bool,
    pub frequency_ok: bool,
    pub valid: bool,
    pub tracker: FrequencyTracker,
    pub polled: bool,
    #[serde(skip)]
    message: message::Reader<OwnedSegments>,
    #[serde(skip)]
    ignore_alive: bool,
    #[serde(skip)]
    ignore_frequency: bool,
    #[serde(skip)]
    ignore_valid: bool,
}

impl Topic {
    pub fn ignores_alive(&self) -> bool {
        self.ignore_alive
    }

    pub fn ignores_valid(&self) -> bool {
        self.ignore_valid
    }

    pub fn ignores_frequency(&self) -> bool {
        self.ignore_frequency || self.ignore_alive
    }

    pub fn event(&self) -> Result<event::Reader<'_>, Error> {
        Ok(self.message.get_root()?)
    }

    pub fn data(&self) -> Result<dynamic_value::Reader<'_>, Error> {
        let dynamic_value::Reader::Struct(event) = self.event()?.into() else {
            return Err(Error::Configuration("Event schema must be a struct"));
        };
        Ok(event.get_named(self.service.name)?)
    }

    pub fn checks_frequency(&self) -> bool {
        self.service.frequency > 0.99 && !self.ignore_frequency && !self.ignore_alive
    }
}

pub struct State {
    frame: i64,
    pub update_frequency: f64,
    topics: Vec<Topic>,
    indices: HashMap<&'static str, usize>,
    simulation: bool,
    ignore_alive: Vec<String>,
    ignore_frequency: Vec<String>,
    ignore_valid: Vec<String>,
}

impl State {
    pub fn new(names: &[&str], options: Options) -> Result<Self, Error> {
        if names.is_empty() {
            return Err(Error::Configuration("at least one service is required"));
        }
        let unique: HashSet<_> = names.iter().copied().collect();
        if unique.len() != names.len() {
            return Err(Error::Configuration("duplicate service"));
        }
        if options.frequency.is_some() && !matches!(options.poll, Poll::All) {
            return Err(Error::Configuration(
                "frequency cannot override explicit poll services",
            ));
        }
        let services: Vec<_> = names
            .iter()
            .map(|name| {
                services::lookup(name).ok_or_else(|| Error::UnknownService((*name).to_owned()))
            })
            .collect::<Result<_, _>>()?;
        let polled: HashSet<&str> = match &options.poll {
            Poll::All => unique.clone(),
            Poll::One(name) => [name.as_str()].into(),
            Poll::Many(names) => names.iter().map(String::as_str).collect(),
        };
        if polled.is_empty() || !polled.is_subset(&unique) {
            return Err(Error::Configuration(
                "poll services must be subscribed and nonempty",
            ));
        }
        let multiple = matches!(options.poll, Poll::Many(_)) && polled.len() > 1;
        let max_poll = services
            .iter()
            .filter(|service| polled.contains(service.name))
            .map(|service| service.frequency)
            .fold(0.0, f64::max);
        let update_frequency = match options.frequency {
            Some(frequency) => frequency,
            None if matches!(options.poll, Poll::Many(_)) => services
                .iter()
                .filter(|service| polled.contains(service.name))
                .map(|service| service.frequency)
                .sum(),
            None => max_poll,
        };
        let mut topics = Vec::with_capacity(services.len());
        let mut indices = HashMap::with_capacity(services.len());
        for service in services {
            let on_demand = service.frequency <= 1e-5;
            let is_polled = polled.contains(service.name);
            indices.insert(service.name, topics.len());
            topics.push(Topic {
                service,
                seen: false,
                updated: false,
                receive_time: 0.0,
                receive_frame: 0,
                log_mono_time: 0,
                alive: on_demand,
                frequency_ok: on_demand,
                valid: on_demand,
                tracker: FrequencyTracker::new(
                    service.frequency,
                    update_frequency,
                    !matches!(options.poll, Poll::All) && is_polled,
                    (multiple && !is_polled).then_some(max_poll),
                    service.frequency_range,
                )?,
                polled: is_polled,
                message: default_message(service.name)?,
                ignore_alive: options.ignore_alive.iter().any(|name| name == service.name),
                ignore_frequency: options
                    .ignore_frequency
                    .iter()
                    .any(|name| name == service.name),
                ignore_valid: options.ignore_valid.iter().any(|name| name == service.name),
            });
        }
        Ok(Self {
            frame: -1,
            update_frequency,
            topics,
            indices,
            simulation: options.simulation,
            ignore_alive: options.ignore_alive,
            ignore_frequency: options.ignore_frequency,
            ignore_valid: options.ignore_valid,
        })
    }

    pub fn append_ignore_alive_valid(&mut self, name: &str) -> Result<(), Error> {
        let index = *self
            .indices
            .get(name)
            .ok_or_else(|| Error::UnknownService(name.to_owned()))?;
        self.topics[index].ignore_alive = true;
        self.topics[index].ignore_valid = true;
        self.ignore_alive.push(name.to_owned());
        self.ignore_valid.push(name.to_owned());
        Ok(())
    }

    /// SelfdriveD supplies one Python list as all three ignore arguments. Appending
    /// through alive and valid aliases adds two entries to each shared list.
    pub fn append_shared_ignore_alive_valid(&mut self, name: &str) -> Result<(), Error> {
        let index = *self
            .indices
            .get(name)
            .ok_or_else(|| Error::UnknownService(name.to_owned()))?;
        self.topics[index].ignore_alive = true;
        self.topics[index].ignore_valid = true;
        self.topics[index].ignore_frequency = true;
        for names in [
            &mut self.ignore_alive,
            &mut self.ignore_valid,
            &mut self.ignore_frequency,
        ] {
            names.extend([name.to_owned(), name.to_owned()]);
        }
        Ok(())
    }

    pub fn ignore_alive(&self) -> &[String] {
        &self.ignore_alive
    }
    pub fn ignore_frequency(&self) -> &[String] {
        &self.ignore_frequency
    }
    pub fn ignore_valid(&self) -> &[String] {
        &self.ignore_valid
    }

    pub fn frame(&self) -> i64 {
        self.frame
    }
    pub fn topics(&self) -> &[Topic] {
        &self.topics
    }

    pub fn topic(&self, name: &str) -> Result<&Topic, Error> {
        self.indices
            .get(name)
            .map(|&index| &self.topics[index])
            .ok_or_else(|| Error::UnknownService(name.to_owned()))
    }

    pub fn update(&mut self, now: f64, messages: &[Vec<u8>]) -> Result<(), Error> {
        self.update_with_clock(messages, || now)
    }

    pub fn update_with_clock(
        &mut self,
        messages: &[Vec<u8>],
        clock: impl FnOnce() -> f64,
    ) -> Result<(), Error> {
        let messages: Vec<_> = messages
            .iter()
            .map(|bytes| read_message(bytes))
            .collect::<Result<_, _>>()?;
        let now = clock();
        self.frame = self.frame.saturating_add(1);
        for topic in &mut self.topics {
            topic.updated = false;
        }
        for message in messages {
            let event: event::Reader<'_> = message.get_root()?;
            let dynamic_value::Reader::Struct(dynamic) = event.into() else {
                return Err(Error::Configuration("Event schema must be a struct"));
            };
            let field = dynamic
                .which()?
                .ok_or(Error::Configuration("missing Event service"))?;
            let name = field.get_proto().get_name()?.to_str()?;
            let index = *self
                .indices
                .get(name)
                .ok_or_else(|| Error::UnknownService(name.to_owned()))?;
            let topic = &mut self.topics[index];
            topic.seen = true;
            topic.updated = true;
            topic.tracker.record(now);
            topic.receive_time = now;
            topic.receive_frame = self.frame;
            topic.log_mono_time = event.get_log_mono_time();
            topic.valid = event.get_valid();
            topic.message = message;
        }
        for topic in &mut self.topics {
            if topic.service.frequency > 1e-5 {
                topic.alive = now - topic.receive_time < 10.0 / topic.service.frequency
                    || (topic.seen && self.simulation);
                topic.frequency_ok = topic.tracker.valid()? || self.simulation;
            }
        }
        Ok(())
    }

    fn all_matching(
        &self,
        names: &[&str],
        predicate: impl Fn(&Topic) -> bool,
    ) -> Result<bool, Error> {
        if names.is_empty() {
            return Ok(self.topics.iter().all(predicate));
        }
        for name in names {
            if !predicate(self.topic(name)?) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub fn all_alive(&self, names: &[&str]) -> Result<bool, Error> {
        self.all_matching(names, |topic| topic.ignore_alive || topic.alive)
    }
    pub fn all_frequency_ok(&self, names: &[&str]) -> Result<bool, Error> {
        self.all_matching(names, |topic| {
            !topic.checks_frequency() || topic.frequency_ok
        })
    }
    pub fn all_valid(&self, names: &[&str]) -> Result<bool, Error> {
        self.all_matching(names, |topic| topic.ignore_valid || topic.valid)
    }
    pub fn all_checks(&self, names: &[&str]) -> Result<bool, Error> {
        Ok(self.all_alive(names)? && self.all_frequency_ok(names)? && self.all_valid(names)?)
    }
}

fn read_message(bytes: &[u8]) -> Result<message::Reader<OwnedSegments>, Error> {
    let reader = serialize::read_message(
        Cursor::new(bytes),
        ReaderOptions {
            traversal_limit_in_words: Some(bytes.len() / 8),
            nesting_limit: 64,
        },
    )?;
    // Bound segment allocation by received bytes, then preserve unlimited repeated source reads.
    let reader = message::Reader::new(
        reader.into_segments(),
        ReaderOptions {
            traversal_limit_in_words: None,
            nesting_limit: 64,
        },
    );
    reader.get_root::<event::Reader<'_>>()?;
    Ok(reader)
}

fn default_message(name: &str) -> Result<message::Reader<OwnedSegments>, Error> {
    let mut message = message::Builder::new_default();
    let mut root: event::Builder<'_> = message.init_root();
    root.set_valid(false);
    let dynamic_value::Builder::Struct(mut dynamic) = root.into() else {
        return Err(Error::Configuration("Event schema must be a struct"));
    };
    if dynamic.reborrow().init_named(name).is_err() {
        dynamic.initn_named(name, 0)?;
    }
    read_message(&serialize::write_message_to_words(&message))
}
