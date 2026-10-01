//! UI Params boundary: raw bytes retain source conversions and write ordering.
use crate::Error;
pub mod binding;
pub mod datetime;
pub mod numeric;
pub mod store;
pub trait Read {
    fn bytes(&self, key: &str) -> Result<Option<Vec<u8>>, Error>;
    fn boolean(&self, key: &str) -> Result<bool, Error> {
        Ok(self.bytes(key)?.as_deref() == Some(b"1"))
    }
    fn integer(&self, key: &str) -> Result<i32, Error> {
        numeric::integer(self.bytes(key)?.as_deref().unwrap_or_default())
            .ok_or_else(|| Error::Parameter(key.into()))
    }
    fn string(&self, key: &str) -> Result<String, Error> {
        let bytes = self.bytes(key)?.unwrap_or_default();
        // Params STRING conversion returns None on invalid UTF-8; callers use an empty fallback.
        Ok(String::from_utf8(bytes).unwrap_or_default())
    }
}
impl Read for openpilot_params::Params {
    fn bytes(&self, key: &str) -> Result<Option<Vec<u8>>, Error> {
        Ok(self.get(key)?)
    }
}

pub mod typed;
