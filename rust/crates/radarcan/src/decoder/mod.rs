mod chrysler;
mod ford;
mod gm;
mod honda;
pub mod hyundai;
mod rivian;
mod tesla;
mod toyota;
mod volkswagen;

use crate::{
    base::Base, data::Data, databases::Databases, integer_set::IntegerSet, numerics::Numerics,
    reader::Reader, scalar, Error,
};
use openpilot_can::Packet;
use serde::{Deserialize, Deserializer};

fn rounded<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f32, D::Error> {
    Ok(scalar::deserialize_float(deserializer)? as f32)
}

#[derive(Clone, Deserialize)]
pub struct Config {
    pub candidate: String,
    #[serde(deserialize_with = "rounded")]
    pub delay: f32,
    #[serde(deserialize_with = "rounded")]
    pub period: f32,
    pub unavailable: bool,
    #[serde(default)]
    pub flags: u32,
    #[serde(default)]
    pub ext_flags: u32,
    #[serde(default = "one")]
    pub safety_count: usize,
}
fn one() -> usize {
    1
}

pub enum Kind {
    Chrysler,
    Gm,
    Ford(ford::Ford),
    Honda(honda::Honda),
    Hyundai(Box<hyundai::Hyundai>),
    Rivian(rivian::Rivian),
    Tesla(tesla::Tesla),
    Toyota(toyota::Toyota),
    Volkswagen(volkswagen::Volkswagen),
    Fallback,
}

pub struct Interface {
    pub base: Base,
    pub kind: Kind,
    pub reader: Option<Reader>,
    pub updated: IntegerSet,
    pub trigger: u32,
    pub unavailable: bool,
}

impl Interface {
    pub fn new(
        config: Config,
        databases: &mut Databases,
        clock: &mut impl FnMut() -> u64,
        emit: &mut impl FnMut(&str),
    ) -> Result<Self, Error> {
        Self::with_settings(
            config,
            &mut hyundai::Environment {
                databases,
                clock,
                emit,
                settings: &mut crate::settings::Unavailable,
            },
        )
    }

    pub fn with_settings<C: FnMut() -> u64, E: FnMut(&str), S: crate::settings::Settings>(
        config: Config,
        environment: &mut hyundai::Environment<'_, C, E, S>,
    ) -> Result<Self, Error> {
        let platform = openpilot_card::vehicle_params::platform(&config.candidate)?;
        let mut base = Base::new(config.delay, config.period)?;
        if platform.brand == "hyundai" {
            let state =
                hyundai::Hyundai::new(&config, &mut base, platform.dbc_pt.as_deref(), environment)?;
            return Ok(Self {
                base,
                kind: Kind::Hyundai(Box::new(state)),
                reader: None,
                updated: IntegerSet::default(),
                trigger: 0,
                unavailable: config.unavailable,
            });
        }
        let (kind, messages, trigger, create) = match platform.brand.as_str() {
            "chrysler" => (Kind::Chrysler, chrysler::messages(), 0x2d4, true),
            "gm" => (Kind::Gm, gm::messages(), 1140, !config.unavailable),
            "honda" => (
                Kind::Honda(honda::Honda::default()),
                honda::messages(),
                0x445,
                !config.unavailable,
            ),
            "rivian" => (
                Kind::Rivian(rivian::Rivian::default()),
                (0x500..0x520).map(|a| (a, 20.)).collect(),
                0x51f,
                true,
            ),
            "tesla" => (
                Kind::Tesla(tesla::Tesla::default()),
                tesla::messages(),
                0x45f,
                true,
            ),
            "toyota" => {
                let state = toyota::Toyota::new(platform.flags & 8 != 0);
                let messages = state.messages();
                let trigger = state.first + 31;
                (Kind::Toyota(state), messages, trigger, !config.unavailable)
            }
            "volkswagen" => (
                Kind::Volkswagen(volkswagen::Volkswagen::default()),
                vec![(0x24f, 25.)],
                0x24f,
                config.flags & 16 != 0 && !config.unavailable,
            ),
            "ford" => {
                let state = ford::Ford::new(platform.dbc_radar.as_deref(), config.unavailable)?;
                let messages = state.messages();
                let trigger = state.trigger();
                (Kind::Ford(state), messages, trigger, !config.unavailable)
            }
            _ => (Kind::Fallback, Vec::new(), 0, false),
        };
        let mandatory = matches!(
            kind,
            Kind::Gm | Kind::Honda(_) | Kind::Rivian(_) | Kind::Toyota(_) | Kind::Ford(_)
        );
        let reader = if create {
            match platform.dbc_radar {
                Some(name) => Some(Reader::new(
                    environment.databases,
                    &name,
                    messages,
                    if matches!(kind, Kind::Ford(_)) {
                        (i64::try_from(config.safety_count).map_err(|_| Error::IntegerOverflow)?
                            - 1)
                        .checked_mul(4)
                        .and_then(|offset| offset.checked_add(1))
                        .ok_or(Error::IntegerOverflow)?
                    } else if matches!(kind, Kind::Volkswagen(_)) {
                        2
                    } else {
                        1
                    },
                    environment.clock,
                    environment.emit,
                )?),
                None if mandatory => return Err(Error::MissingRadarDbc(config.candidate)),
                None => None,
            }
        } else {
            None
        };
        Ok(Self {
            base,
            kind,
            reader,
            updated: IntegerSet::default(),
            trigger,
            unavailable: config.unavailable,
        })
    }

    pub fn update(
        &mut self,
        packets: &[Packet],
        numerics: &Numerics,
    ) -> Result<Option<Data>, Error> {
        if let Kind::Hyundai(state) = &mut self.kind {
            return state.update(&mut self.base, packets);
        }
        if self.unavailable || self.reader.is_none() {
            return Ok(self.base.fallback());
        }
        let reader = self
            .reader
            .as_mut()
            .ok_or(Error::Contract("active radar reader absent"))?;
        reader.update(packets, &mut self.updated)?;
        if !self.updated.contains(self.trigger) {
            return Ok(None);
        }
        let data = match &mut self.kind {
            Kind::Chrysler => chrysler::update(&mut self.base, reader, &self.updated)?,
            Kind::Gm => gm::update(&mut self.base, reader, &self.updated)?,
            Kind::Ford(state) => {
                return state.update(&mut self.base, reader, &mut self.updated, numerics)
            }
            Kind::Honda(state) => state.update(&mut self.base, reader, &self.updated)?,
            Kind::Hyundai(_) => {
                return Err(Error::Contract(
                    "Hyundai unexpectedly has shared radar reader",
                ))
            }
            Kind::Rivian(state) => state.update(&mut self.base, reader)?,
            Kind::Tesla(state) => state.update(&mut self.base, reader)?,
            Kind::Toyota(state) => state.update(&mut self.base, reader, &self.updated)?,
            Kind::Volkswagen(state) => state.update(&mut self.base, reader)?,
            Kind::Fallback => {
                return Err(Error::Contract("fallback unexpectedly has radar reader"))
            }
        };
        self.updated.clear();
        Ok(Some(data))
    }

    pub fn update_carrot(
        &mut self,
        v_ego: f64,
        a_ego: f64,
        time: f64,
        packets: &[Packet],
        numerics: &mut Numerics,
        emit: &mut impl FnMut(&str),
    ) -> Result<Option<Data>, Error> {
        self.base.push_ego(v_ego, a_ego)?;
        let result = self.update(packets, numerics)?;
        self.base.finish(result, time, numerics, emit)
    }
}
