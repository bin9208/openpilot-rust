use crate::{parameters::Parameters, Error};
use openpilot_params::Params;

pub struct RuntimeParameters(pub Params);

impl RuntimeParameters {
    fn raw(&self, key: &str) -> Result<Vec<u8>, Error> {
        match self.0.get(key) {
            Ok(value) => Ok(value.unwrap_or_default()),
            Err(openpilot_params::Error::Io(_)) => Ok(Vec::new()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn car_params(&self) -> Result<Vec<u8>, Error> {
        self.raw("CarParams")
    }
}

pub fn integer(bytes: &[u8]) -> Result<i32, Error> {
    Ok(openpilot_beepd::integer(bytes)?)
}
pub fn float(bytes: &[u8]) -> Result<f64, Error> {
    Ok(openpilot_calibrationd::parameters::parse_float(bytes)?)
}

impl Parameters for RuntimeParameters {
    fn integer(&mut self, key: &'static str) -> Result<i32, Error> {
        integer(&self.raw(key)?)
    }
    fn float(&mut self, key: &'static str) -> Result<f64, Error> {
        float(&self.raw(key)?)
    }
}
