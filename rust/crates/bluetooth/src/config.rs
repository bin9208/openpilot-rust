use crate::{Action, Mapping, Profile, Token};
use indexmap::IndexMap;
use openpilot_logmessaged::{JsonValue, JsonView};
use serde::{Serialize, Serializer};
use std::collections::HashSet;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("devices must be an object")]
    Devices,
    #[error("at most 16 remotes")]
    DeviceCount,
    #[error("invalid Bluetooth address")]
    Address,
    #[error("unknown input profile")]
    Profile,
    #[error("invalid mapping")]
    Mapping,
    #[error("invalid button or action")]
    Button,
    #[error("invalid Linux key code")]
    KeyCode,
    #[error("enabled must be boolean")]
    Enabled,
    #[error("invalid Unicode code point")]
    Unicode,
    #[error(transparent)]
    Json(#[from] openpilot_logmessaged::JsonError),
    #[error(transparent)]
    Text(#[from] openpilot_runtime_version::Error),
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Address(String);

impl Address {
    pub fn parse(value: &str) -> Result<Self, ConfigError> {
        let value = value.to_uppercase();
        let parts: Vec<_> = value.split(':').collect();
        if parts.len() != 6
            || parts
                .iter()
                .any(|part| part.len() != 2 || !part.bytes().all(|byte| byte.is_ascii_hexdigit()))
        {
            return Err(ConfigError::Address);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone)]
pub struct Name(JsonValue);

impl PartialEq for Name {
    fn eq(&self, other: &Self) -> bool {
        match (self.0.view(), other.0.view()) {
            (JsonView::Text(left), JsonView::Text(right)) => left == right,
            _ => false,
        }
    }
}

impl Eq for Name {}

impl Serialize for Name {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let json = self.0.to_json().map_err(serde::ser::Error::custom)?;
        let raw =
            serde_json::value::RawValue::from_string(json).map_err(serde::ser::Error::custom)?;
        raw.serialize(serializer)
    }
}

#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct Device {
    pub name: Name,
    pub profile: Profile,
    pub enabled: bool,
    pub mapping: Mapping,
}

#[derive(Clone, Default, Serialize)]
pub struct Config {
    version: Version,
    pub devices: IndexMap<Address, Device>,
}

#[derive(Clone, Default)]
struct Version;

impl Serialize for Version {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(1)
    }
}

impl Config {
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        let data = JsonValue::parse(text)?;
        if !data.is_object() {
            return Err(ConfigError::Devices);
        }
        let Some(devices) = data.get("devices") else {
            return Ok(Self::default());
        };
        let JsonView::Object(devices) = devices.view() else {
            return Err(ConfigError::Devices);
        };
        if devices.len() > 16 {
            return Err(ConfigError::DeviceCount);
        }
        let mut result = Self::default();
        for (address, device) in devices {
            let text: String = address
                .iter()
                .copied()
                .map(char::from_u32)
                .collect::<Option<_>>()
                .ok_or(ConfigError::Address)?;
            let address = Address::parse(&text)?;
            let Some(profile) = device.get("profile") else {
                return Err(ConfigError::Profile);
            };
            let profile = match profile.to_utf8().as_deref() {
                Some("yiser-j6") => Profile::YiserJ6,
                Some("generic") => Profile::Generic,
                _ => return Err(ConfigError::Profile),
            };
            let mapping = mapping(device.get("mapping"))?;
            let enabled = match device.get("enabled").as_ref().map(JsonValue::view) {
                None => false,
                Some(JsonView::Bool(value)) => value,
                _ => return Err(ConfigError::Enabled),
            };
            let source_name = device
                .get("name")
                .unwrap_or_else(|| JsonValue::text(address.as_str()));
            let mut points = openpilot_runtime_version::python_str(&source_name)?;
            points.truncate(80);
            let name = Name(JsonValue::codepoints(points).ok_or(ConfigError::Unicode)?);
            result.devices.insert(
                address,
                Device {
                    name,
                    profile,
                    enabled,
                    mapping,
                },
            );
        }
        Ok(result)
    }
}

fn mapping(value: Option<JsonValue>) -> Result<Mapping, ConfigError> {
    let Some(value) = value else {
        return Ok(Mapping::default());
    };
    let JsonView::Object(fields) = value.view() else {
        return Err(ConfigError::Mapping);
    };
    if fields.len() > 192 {
        return Err(ConfigError::Mapping);
    }
    let mut entries = Vec::with_capacity(fields.len());
    let mut bases = HashSet::new();
    for (key, value) in fields {
        bases.insert(
            key.split(|point| *point == u32::from('@'))
                .next()
                .unwrap_or_default()
                .to_vec(),
        );
        entries.push((key, value));
    }
    if bases.len() > 64 {
        return Err(ConfigError::Mapping);
    }
    let mut result = Mapping::default();
    for (key, value) in entries {
        let key = key
            .iter()
            .copied()
            .map(char::from_u32)
            .collect::<Option<String>>()
            .ok_or(ConfigError::Button)?;
        let text = value.to_utf8().ok_or(ConfigError::Button)?;
        let action: Action = serde::Deserialize::deserialize(serde::de::value::StrDeserializer::<
            serde::de::value::Error,
        >::new(&text))
        .map_err(|_| ConfigError::Button)?;
        let token = Token::parse_mapping(&key)?;
        result.0.insert(token, action);
    }
    Ok(result)
}
