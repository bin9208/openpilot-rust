use super::{Error, ADAPTER, DEVICE};
use dbus::{
    arg::{Dict, Iter, PropMap},
    Message, Path,
};
use serde::Serialize;
use std::collections::HashMap;

pub(super) type Object = (Path<'static>, HashMap<String, PropMap>);

pub(super) fn read(message: &Message) -> Result<Vec<Object>, Error> {
    let entries: Dict<'_, Path<'_>, HashMap<String, PropMap>, Iter<'_>> = message.read1()?;
    Ok(entries
        .map(|(path, interfaces)| (path.into_static(), interfaces))
        .collect())
}

#[derive(Clone, Serialize)]
pub struct Adapter {
    pub address: Option<String>,
    pub powered: bool,
    pub discovering: bool,
}

#[derive(Clone, Serialize)]
pub struct Device {
    pub address: Option<String>,
    pub name: String,
    pub paired: bool,
    pub connected: bool,
    pub trusted: bool,
    pub uuids: Vec<String>,
    pub rssi: Option<i64>,
    pub battery: Option<u64>,
}

fn text(values: &PropMap, key: &'static str) -> Result<Option<String>, Error> {
    values
        .get(key)
        .map(|value| {
            value
                .0
                .as_str()
                .map(str::to_owned)
                .ok_or(Error::Property(key))
        })
        .transpose()
}

fn boolean(values: &PropMap, key: &'static str) -> Result<bool, Error> {
    values
        .get(key)
        .map(|value| {
            value
                .0
                .as_i64()
                .map(|value| value != 0)
                .ok_or(Error::Property(key))
        })
        .transpose()
        .map(|value| value.unwrap_or(false))
}

pub(super) fn snapshot(objects: &[Object]) -> Result<(Vec<Adapter>, Vec<Device>), Error> {
    let mut adapters = Vec::new();
    let mut devices = Vec::new();
    for (_, interfaces) in objects {
        if let Some(props) = interfaces.get(ADAPTER) {
            adapters.push(Adapter {
                address: text(props, "Address")?,
                powered: boolean(props, "Powered")?,
                discovering: boolean(props, "Discovering")?,
            });
        }
        if let Some(props) = interfaces.get(DEVICE) {
            let name = match text(props, "Name")? {
                Some(name) => name,
                None => text(props, "Alias")?.unwrap_or_default(),
            };
            let uuids = props
                .get("UUIDs")
                .map(|value| {
                    value
                        .0
                        .as_iter()
                        .ok_or(Error::Property("UUIDs"))?
                        .map(|value| {
                            value
                                .as_str()
                                .map(str::to_owned)
                                .ok_or(Error::Property("UUIDs"))
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?
                .unwrap_or_default();
            let rssi = props
                .get("RSSI")
                .map(|value| value.0.as_i64().ok_or(Error::Property("RSSI")))
                .transpose()?;
            let battery = interfaces
                .get("org.bluez.Battery1")
                .and_then(|props| props.get("Percentage"))
                .map(|value| value.0.as_u64().ok_or(Error::Property("Percentage")))
                .transpose()?;
            devices.push(Device {
                address: text(props, "Address")?,
                name,
                paired: boolean(props, "Paired")?,
                connected: boolean(props, "Connected")?,
                trusted: boolean(props, "Trusted")?,
                uuids,
                rssi,
                battery,
            });
        }
    }
    Ok((adapters, devices))
}

pub(super) fn address(properties: &HashMap<String, PropMap>) -> Result<Option<String>, Error> {
    properties
        .get(DEVICE)
        .map(|values| text(values, "Address"))
        .transpose()
        .map(Option::flatten)
}
