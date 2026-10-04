use crate::{parameters::Parameters, Error};
use openpilot_params::Params;

pub struct RuntimeParameters(pub Params);

pub fn integer(bytes: &[u8]) -> Result<i32, Error> {
    Ok(openpilot_beepd::integer(bytes)?)
}
pub fn float(bytes: &[u8]) -> Result<f64, Error> {
    Ok(openpilot_calibrationd::parameters::parse_float(bytes)?)
}

impl Parameters for RuntimeParameters {
    fn integer(&mut self, key: &'static str) -> Result<i32, Error> {
        integer(self.0.get(key)?.as_deref().unwrap_or_default())
    }
    fn float(&mut self, key: &'static str) -> Result<f64, Error> {
        float(self.0.get(key)?.as_deref().unwrap_or_default())
    }
}
