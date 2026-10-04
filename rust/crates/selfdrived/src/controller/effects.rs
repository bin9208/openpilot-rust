use super::Error;
use crate::{callbacks::AlertParams, car_specific::CarSpecificParams};
use openpilot_cereal::log_capnp::LongitudinalPersonality;
use openpilot_logging::Fields;
use openpilot_msgq::VisionStream;
use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(transparent)]
pub struct Personality(pub String);
impl Personality {
    pub fn standard() -> Self {
        Self("1".into())
    }
    pub fn wire(&self) -> Result<LongitudinalPersonality, Error> {
        match self.0.as_str() {
            "0" => Ok(LongitudinalPersonality::Aggressive),
            "1" => Ok(LongitudinalPersonality::Standard),
            "2" => Ok(LongitudinalPersonality::Relaxed),
            "3" => Ok(LongitudinalPersonality::MoreRelaxed),
            _ => Err(Error::Personality(self.0.clone())),
        }
    }
}

pub trait Effects: AlertParams + CarSpecificParams<Error = crate::callbacks::Error> {
    fn setup_publishers(&mut self) -> Result<(), Error> {
        Ok(())
    }
    fn setup_subscribers(
        &mut self,
        _names: &[&str],
        _options: openpilot_messaging::state::Options,
    ) -> Result<(), Error> {
        Ok(())
    }
    fn presence(&mut self, key: &str) -> Result<bool, Error>;
    fn wide_camera(&mut self) -> Result<bool, Error>;
    fn personality(&mut self) -> Result<Personality, Error>;
    fn remove(&mut self, key: &str) -> Result<(), Error>;
    fn offroad(&mut self, key: &str, extra: Option<&str>) -> Result<(), Error>;
    fn event(&mut self, name: &str, fields: Fields) -> Result<(), Error>;
    fn monotonic(&mut self) -> f64;
    fn timestamp(&mut self) -> Result<u64, Error>;
    fn available_streams(&mut self) -> Result<Vec<VisionStream>, Error>;
}

pub trait Publications {
    fn send(&mut self, topic: &str, bytes: &[u8]) -> Result<(), Error>;
}
