use crate::Error;
use openpilot_params::Params;

pub trait Settings {
    fn integer(&mut self, key: &'static str) -> Result<i32, Error>;
}

pub struct Native(pub Params);

impl Native {
    pub(crate) fn raw(&self, key: &str) -> Result<Vec<u8>, Error> {
        match self.0.get(key) {
            Ok(value) => Ok(value.unwrap_or_default()),
            Err(openpilot_params::Error::Io(_)) => Ok(Vec::new()),
            Err(error) => Err(error.into()),
        }
    }
}

impl Settings for Native {
    fn integer(&mut self, key: &'static str) -> Result<i32, Error> {
        let bytes = self.raw(key)?;
        openpilot_beepd::integer(&bytes).map_err(|error| Error::ParameterInteger { key, error })
    }
}

pub struct Unavailable;
impl Settings for Unavailable {
    fn integer(&mut self, _key: &'static str) -> Result<i32, Error> {
        Err(Error::Contract(
            "Params reader required for Hyundai radar constructor",
        ))
    }
}
