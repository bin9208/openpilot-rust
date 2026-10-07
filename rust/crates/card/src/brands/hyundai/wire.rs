use super::Error;
use num_traits::ToPrimitive;
use openpilot_can::{dbc::Dbc, packer::Packer, Frame};
use std::{collections::BTreeMap, path::Path, sync::Arc};

pub type Values = BTreeMap<String, f64>;

pub struct CanWriter {
    pub packer: Packer,
}

impl CanWriter {
    pub fn new(path: &Path) -> Result<Self, Error> {
        Ok(Self::from_dbc(Arc::new(Dbc::load(path)?)))
    }

    pub fn from_dbc(dbc: Arc<Dbc>) -> Self {
        Self {
            packer: Packer::new(dbc),
        }
    }

    pub fn frame(
        &mut self,
        name: &str,
        bus: u8,
        values: &Values,
        counter: Option<i64>,
    ) -> Result<Frame, Error> {
        let address = self.packer.dbc.message(name)?.address;
        let signals: Vec<_> = values
            .iter()
            .map(|(name, value)| (name.as_str(), *value))
            .collect();
        Ok(Frame {
            address,
            data: self.packer.pack(name, &signals, counter)?,
            bus,
        })
    }
}

pub fn values(entries: &[(&str, f64)]) -> Values {
    entries
        .iter()
        .map(|(name, value)| ((*name).to_owned(), *value))
        .collect()
}

pub fn set(target: &mut Values, entries: &[(&str, f64)]) {
    target.extend(
        entries
            .iter()
            .map(|(name, value)| ((*name).to_owned(), *value)),
    );
}

pub fn get(target: &Values, name: &str) -> Result<f64, Error> {
    target
        .get(name)
        .copied()
        .ok_or_else(|| Error::Signal(name.to_owned()))
}

pub fn copy_signals(target: &Values, names: &[&str]) -> Result<Values, Error> {
    names
        .iter()
        .map(|name| Ok(((*name).to_owned(), get(target, name)?)))
        .collect()
}

pub fn remove_counter(target: &mut Values) -> Result<Option<i64>, Error> {
    target
        .remove("COUNTER")
        .map(|value| value.to_i64().ok_or(Error::Numeric))
        .transpose()
}

pub fn crc8(data: &[u8], polynomial: u8, init: u8, xor: u8) -> u8 {
    let mut crc = init;
    for byte in data {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ polynomial
            } else {
                crc << 1
            };
        }
    }
    crc ^ xor
}

pub fn boolean(value: bool) -> f64 {
    f64::from(u8::from(value))
}
