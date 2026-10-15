use crate::Error;
use std::{ffi::OsStr, fs, path::Path};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Auto,
    Usb,
    Android,
    Ios,
}
impl Mode {
    pub fn parse(value: &str) -> Result<Self, Error> {
        match value {
            "auto" => Ok(Self::Auto),
            "usb" => Ok(Self::Usb),
            "android" => Ok(Self::Android),
            "ios" => Ok(Self::Ios),
            _ => Err(Error::Contract(
                "JETLINK_TRANSPORT must be auto, usb, android or ios",
            )),
        }
    }
    pub fn composite(self) -> bool {
        matches!(self, Self::Auto | Self::Ios)
    }
    pub fn read(override_value: Option<&OsStr>, path: &Path) -> Result<Self, Error> {
        if let Some(value) = override_value {
            return Self::parse(
                value
                    .to_str()
                    .ok_or(Error::Contract("transport mode encoding"))?,
            );
        }
        match fs::read_to_string(path) {
            Ok(value) => {
                Self::parse(value.trim_matches(|c: char| {
                    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
                }))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::Auto),
            Err(error) => Err(error.into()),
        }
    }
    pub fn device() -> Result<Self, Error> {
        Self::read(
            std::env::var_os("JETLINK_TRANSPORT").as_deref(),
            Path::new("/data/jetlink-transport"),
        )
    }
}
