use openpilot_radarcan::{settings::Settings, Error};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Observed {
    pub values: Option<BTreeMap<String, String>>,
    pub reads: Vec<Read>,
}

#[derive(Serialize)]
pub struct Read {
    key: &'static str,
    value: i32,
}

impl Settings for Observed {
    fn integer(&mut self, key: &'static str) -> Result<i32, Error> {
        let values = self
            .values
            .as_ref()
            .ok_or(Error::Contract("fixture Params required"))?;
        let bytes = values.get(key).map_or(&[][..], |value| value.as_bytes());
        let value = openpilot_beepd::integer(bytes)
            .map_err(|error| Error::ParameterInteger { key, error })?;
        self.reads.push(Read { key, value });
        Ok(value)
    }
}
