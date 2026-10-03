use crate::{
    activity::{Activity, Emission},
    atomic_json, atomic_value,
    learning::Learning,
    Address, Channel, CommandWriter, Config, Decoder, Device, Error, Event, Gates, Seconds, Token,
    VehicleSnapshot,
};
use indexmap::IndexMap;
use openpilot_logmessaged::JsonValue;
use serde::Serialize;
use std::{
    collections::{BTreeSet, HashSet},
    path::{Path, PathBuf},
};

pub(crate) struct Remote {
    pub address: Address,
    pub device: Device,
    pub decoder: Decoder,
}

pub struct Reload {
    pub close: Vec<String>,
    pub open: Vec<(String, Address)>,
}

pub struct Engine {
    root: PathBuf,
    settings: Config,
    learning: Learning,
    remotes: IndexMap<String, Remote>,
    gates: Gates,
    activity: Activity,
    errors: IndexMap<Address, String>,
    writer: CommandWriter,
}

impl Engine {
    pub fn new(root: &Path, settings: Config) -> Result<Self, Error> {
        let writer = CommandWriter::new(root);
        for channel in Channel::ALL {
            writer.publish(channel)?;
        }
        Ok(Self {
            root: root.to_owned(),
            settings,
            learning: Learning::default(),
            remotes: IndexMap::new(),
            gates: Gates::default(),
            activity: Activity::default(),
            errors: IndexMap::new(),
            writer,
        })
    }

    pub fn update(&mut self, state: VehicleSnapshot) {
        self.gates.update(state);
    }

    pub const fn gates(&self) -> Gates {
        self.gates
    }

    pub fn reload(
        &mut self,
        updated: Config,
        value: Option<JsonValue>,
        available: &IndexMap<String, Address>,
        now: Seconds,
    ) -> Reload {
        let learning = Learning::new(value, now);
        let close: Vec<_> = self
            .remotes
            .iter()
            .filter_map(|(path, remote)| {
                let new = updated.devices.get(&remote.address);
                let wanted =
                    new.is_some_and(|device| device.enabled || learning.matches(&remote.address));
                let changed =
                    self.learning.matches(&remote.address) != learning.matches(&remote.address);
                (!available.contains_key(path) || !wanted || Some(&remote.device) != new || changed)
                    .then(|| path.clone())
            })
            .collect();
        for path in &close {
            self.remotes.shift_remove(path);
        }
        self.settings = updated;
        self.learning = learning;
        self.errors.clear();
        let open = available
            .iter()
            .filter(|(path, address)| {
                !self.remotes.contains_key(*path)
                    && self
                        .settings
                        .devices
                        .get(*address)
                        .is_some_and(|device| device.enabled || self.learning.matches(address))
            })
            .map(|(path, address)| (path.clone(), address.clone()))
            .collect();
        Reload { close, open }
    }

    pub fn connect(&mut self, path: &str, address: &Address) -> Result<(), Error> {
        let device = self
            .settings
            .devices
            .get(address)
            .ok_or_else(|| Error::Unconfigured(address.as_str().to_owned()))?
            .clone();
        let decoder = Decoder::new(
            device.profile,
            device.mapping.clone(),
            self.learning.matches(address),
        );
        self.remotes.insert(
            path.to_owned(),
            Remote {
                address: address.clone(),
                device,
                decoder,
            },
        );
        Ok(())
    }

    pub fn open_error(&mut self, address: Address, error: String) {
        self.errors.insert(address, error);
    }

    pub fn disconnect(&mut self, path: &str) {
        self.remotes.shift_remove(path);
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.remotes.keys().map(String::as_str)
    }

    pub fn event(&mut self, path: &str, event: Event, observed: Seconds) -> Result<(), Error> {
        let remote = self
            .remotes
            .get_mut(path)
            .ok_or_else(|| Error::NotOpen(path.to_owned()))?;
        if !(0.0..0.4).contains(&(observed.0 - event.at.0)) {
            remote.decoder.feed(Event {
                kind: 0,
                code: 3,
                value: 0,
                at: observed,
            });
            return Ok(());
        }
        let tokens = remote.decoder.feed(event);
        self.emit(path, &tokens, observed)
    }

    pub fn flush(
        &mut self,
        path: &str,
        decoder_at: Seconds,
        emitted_at: Seconds,
    ) -> Result<(), Error> {
        let tokens = self
            .remotes
            .get_mut(path)
            .ok_or_else(|| Error::NotOpen(path.to_owned()))?
            .decoder
            .flush(decoder_at);
        self.emit(path, &tokens, emitted_at)
    }

    fn emit(&mut self, path: &str, tokens: &[Token], at: Seconds) -> Result<(), Error> {
        let remote = self
            .remotes
            .get_mut(path)
            .ok_or_else(|| Error::NotOpen(path.to_owned()))?;
        self.activity.emit(
            remote,
            Emission {
                path,
                tokens,
                at,
                gates: self.gates,
                learning: &self.learning,
                writer: &mut self.writer,
            },
        )
    }

    pub fn prune(&mut self, now: Seconds) -> Result<(), Error> {
        let addresses: HashSet<_> = self
            .remotes
            .values()
            .filter(|remote| {
                self.gates.started
                    && self.gates.car_ok
                    && remote.device.enabled
                    && !self.learning.matches(&remote.address)
            })
            .map(|remote| remote.address.clone())
            .collect();
        let holds = self
            .remotes
            .iter()
            .flat_map(|(path, remote)| {
                remote
                    .decoder
                    .active_longs()
                    .into_iter()
                    .map(move |token| format!("{path}:{}", token.as_str()))
            })
            .collect();
        self.writer.prune(&addresses, now, Some(&holds))
    }

    pub fn write_status(&self, now: Seconds) -> Result<(), Error> {
        #[derive(Serialize)]
        struct Status<'a> {
            time: Seconds,
            stationary: bool,
            started: bool,
            grabbed: BTreeSet<&'a str>,
            errors: &'a IndexMap<Address, String>,
            last_event: &'a Option<crate::activity::Record>,
            last_events: &'a IndexMap<Address, crate::activity::Record>,
            recent_events: &'a std::collections::VecDeque<crate::activity::Record>,
        }
        let status = Status {
            time: now,
            stationary: self.gates.stationary,
            started: self.gates.started,
            grabbed: self
                .remotes
                .values()
                .map(|remote| remote.address.as_str())
                .collect(),
            errors: &self.errors,
            last_event: &self.activity.last,
            last_events: &self.activity.by_address,
            recent_events: &self.activity.recent,
        };
        let mut json = serde_json::to_string(&status)?;
        json.pop();
        use std::fmt::Write;
        write!(json, ",\"learning\":{}}}", self.learning.json()?)?;
        atomic_value(&self.root.join("status.json"), &JsonValue::parse(&json)?)
    }

    pub fn stopped(&self, now: Seconds) -> Result<(), Error> {
        atomic_json(
            &self.root.join("status.json"),
            &serde_json::json!({"time":now,"stationary":false,"grabbed":[],"stopped":true}),
        )
    }
}
