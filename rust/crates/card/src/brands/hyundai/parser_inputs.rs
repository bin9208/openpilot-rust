use super::{config::CarConfig, flags as f, wire::Values, Error};
use openpilot_can::{dbc::Dbc, parser::Parser, Packet};
use std::{collections::BTreeMap, path::Path, sync::Arc};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Pt,
    Cam,
    Alt,
}

pub struct Inputs {
    pub diagnostics: super::diagnostics::Diagnostics,
    pub pt: Parser,
    pub cam: Parser,
    pub alt: Option<Parser>,
    pub captures: BTreeMap<&'static str, (Channel, &'static str)>,
}

impl Inputs {
    pub fn new(path: &Path, config: &CarConfig, now: u64) -> Result<Self, Error> {
        let dbc = Arc::new(Dbc::load(path)?);
        let fd = config.flags & f::CANFD != 0;
        let mut pt = Parser::new(Arc::clone(&dbc), if fd { config.bus.ecan } else { 0 }, now);
        let mut alt = if fd {
            Some(Parser::new(Arc::clone(&dbc), config.bus.acan, now))
        } else {
            None
        };
        if fd {
            let names = if config.candidate == "KIA_PV5" {
                &["CANFD_HDA_INFO_364", "CANFD_NAVI_PROFILE_093"][..]
            } else {
                &["NEW_MSG_4B4", "NEW_MSG_4B9", "NEW_MSG_4BE"][..]
            };
            for name in names {
                pt.add(name, Some(f64::NAN), false, now)?;
            }
            if config.candidate == "KIA_PV5" {
                if let Some(parser) = &mut alt {
                    parser.add("CANFD_NAVI_STATUS_380", Some(f64::NAN), false, now)?;
                }
            }
            if config.flags & f::ALT_BUTTONS == 0 {
                pt.add("CRUISE_BUTTONS", Some(50.), false, now)?;
            }
            if config.ext_flags & f::ext::EV_MODE_230 != 0 {
                pt.add("HCU_STATUS_230", Some(f64::NAN), true, now)?;
            }
        } else {
            pt.add("EMS21", Some(f64::NAN), false, now)?;
        }
        Ok(Self {
            diagnostics: super::diagnostics::Diagnostics::default(),
            pt,
            cam: Parser::new(dbc, if fd { config.bus.cam } else { 2 }, now),
            alt,
            captures: BTreeMap::new(),
        })
    }

    pub fn parser(&self, channel: Channel) -> Result<&Parser, Error> {
        match channel {
            Channel::Pt => Ok(&self.pt),
            Channel::Cam => Ok(&self.cam),
            Channel::Alt => self
                .alt
                .as_ref()
                .ok_or_else(|| Error::Signal("alt CAN parser".into())),
        }
    }

    pub fn parser_mut(&mut self, channel: Channel) -> Result<&mut Parser, Error> {
        match channel {
            Channel::Pt => Ok(&mut self.pt),
            Channel::Cam => Ok(&mut self.cam),
            Channel::Alt => self
                .alt
                .as_mut()
                .ok_or_else(|| Error::Signal("alt CAN parser".into())),
        }
    }

    pub fn update(&mut self, packets: &[Packet]) -> Result<(), Error> {
        self.diagnostics.seen((&self.pt, Channel::Pt), packets)?;
        self.pt.update(packets)?;
        self.diagnostics.retain_warnings(&mut self.pt);
        self.diagnostics.seen((&self.cam, Channel::Cam), packets)?;
        self.cam.update(packets)?;
        self.diagnostics.retain_warnings(&mut self.cam);
        if let Some(alt) = &mut self.alt {
            self.diagnostics.seen((alt, Channel::Alt), packets)?;
            alt.update(packets)?;
            self.diagnostics.retain_warnings(alt);
        }
        Ok(())
    }

    pub fn signal(
        &mut self,
        channel: Channel,
        name: &str,
        signal: &str,
        now: u64,
    ) -> Result<f64, Error> {
        Ok(self.parser_mut(channel)?.signal_lazy(name, signal, now)?)
    }

    pub fn all(
        &mut self,
        channel: Channel,
        name: &str,
        signal: &str,
        now: u64,
    ) -> Result<Vec<f64>, Error> {
        let parser = self.parser_mut(channel)?;
        parser.signal_lazy(name, signal, now)?;
        let message = parser.dbc.message(name)?;
        let index = message
            .signals
            .iter()
            .position(|s| s.name == signal)
            .ok_or_else(|| Error::Signal(signal.into()))?;
        parser
            .states
            .get(&message.address)
            .and_then(|state| state.all_values.get(index))
            .cloned()
            .ok_or(Error::Numeric)
    }

    pub fn values(&self, channel: Channel, name: &str) -> Result<Values, Error> {
        let parser = self.parser(channel)?;
        let message = parser.dbc.message(name)?;
        let state = parser
            .states
            .get(&message.address)
            .ok_or_else(|| Error::Signal(name.into()))?;
        Ok(message
            .signals
            .iter()
            .zip(&state.values)
            .map(|(signal, value)| (signal.name.clone(), *value))
            .collect())
    }

    pub fn captured(&self, key: &str) -> Result<Option<Values>, Error> {
        self.captures
            .get(key)
            .map(|(channel, name)| self.values(*channel, name))
            .transpose()
    }

    pub fn capture(
        &mut self,
        input: (Channel, &'static str, &'static str),
        ignore_counter: bool,
        now: u64,
    ) -> Result<bool, Error> {
        let (channel, name, key) = input;
        let parser = self.parser_mut(channel)?;
        let Some(address) = parser.dbc.names.get(name).copied() else {
            self.diagnostics.prints.push(format!("{name} not in DBC"));
            return Ok(false);
        };
        if parser.seen_addresses.contains(&address) && !parser.states.contains_key(&address) {
            parser.add(name, None, ignore_counter, now)?;
        }
        if parser.states.contains_key(&address) {
            self.captures.insert(key, (channel, name));
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn validity(&mut self) -> [bool; 2] {
        let mut valid = self.pt.can_valid();
        self.diagnostics.retain_warnings(&mut self.pt);
        if valid {
            valid = self.cam.can_valid();
            self.diagnostics.retain_warnings(&mut self.cam);
        }
        if let Some(alt) = &mut self.alt {
            if valid {
                valid = alt.can_valid();
                self.diagnostics.retain_warnings(alt);
            }
        }
        let timeout = self.pt.bus_timeout()
            || self.cam.bus_timeout()
            || self.alt.as_ref().is_some_and(Parser::bus_timeout);
        [valid, timeout]
    }
}
