use crate::{json::Value, Error};
use num_traits::ToPrimitive;

#[derive(Clone)]
pub struct Options {
    pub host: String,
    pub port: u16,
    pub advertise_ip: Option<String>,
    pub beacon: bool,
    pub cereal: bool,
    pub map_theme: Option<String>,
    pub map_type: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".into(),
            port: 7714,
            advertise_ip: None,
            beacon: true,
            cereal: true,
            map_theme: None,
            map_type: None,
        }
    }
}

impl Options {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Option<Self>, Error> {
        let mut options = Self::default();
        let mut args = args.into_iter();
        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--help" | "-h" => return Ok(None),
                "--no-beacon" => options.beacon = false,
                "--no-cereal" => options.cereal = false,
                "--host" | "--port" | "--advertise-ip" | "--map-theme" | "--map-type" => {
                    let value = args.next().ok_or_else(|| {
                        Error::value(&format!("argument {argument}: expected one argument"))
                    })?;
                    match argument.as_str() {
                        "--host" => options.host = value,
                        "--port" => {
                            options.port = Value::text(&value).int()?.to_u16().ok_or_else(|| {
                                Error::typed(
                                    "OverflowError",
                                    "bind(): port must be 0-65535.".into(),
                                )
                            })?
                        }
                        "--advertise-ip" => options.advertise_ip = Some(value),
                        "--map-theme" if ["auto", "dark", "light"].contains(&value.as_str()) => {
                            options.map_theme = Some(value)
                        }
                        "--map-type" if ["normal", "satellite"].contains(&value.as_str()) => {
                            options.map_type = Some(value)
                        }
                        _ => {
                            return Err(Error::value(&format!(
                                "argument {argument}: invalid choice: {value}"
                            )))
                        }
                    }
                }
                _ => return Err(Error::value(&format!("unrecognized arguments: {argument}"))),
            }
        }
        Ok(Some(options))
    }
    pub fn advertised(&self) -> Option<&str> {
        self.advertise_ip
            .as_deref()
            .filter(|ip| !ip.is_empty())
            .or_else(|| {
                (!matches!(self.host.as_str(), "" | "0.0.0.0" | "::")).then_some(self.host.as_str())
            })
    }
}
