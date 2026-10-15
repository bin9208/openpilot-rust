//! Change framing only by closing/reopening after HELLO failure.
use crate::{wire::Version, Error};
#[derive(Clone, Copy, Debug)]
pub struct ProtocolChoice {
    pub version: Version,
    automatic: bool,
}
impl ProtocolChoice {
    pub fn parse(setting: &str) -> Result<Self, Error> {
        let (version, automatic) = match setting {
            "auto" => (Version::V3, true),
            "2" => (Version::V2, false),
            "3" => (Version::V3, false),
            _ => return Err(Error::Contract("JETLINK_PROTOCOL must be auto, 2 or 3")),
        };
        Ok(Self { version, automatic })
    }
    /// Call only after the failed HELLO's transport has been closed.
    pub fn handshake_failed(&mut self) {
        if self.automatic {
            self.version = match self.version {
                Version::V2 => Version::V3,
                Version::V3 => Version::V2,
            };
        }
    }
}
