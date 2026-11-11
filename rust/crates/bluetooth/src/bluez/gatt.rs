//! GATT characteristic discovery and value transfer for the Carrot-B1 board
//! (carrot `b1_monitor.discover` / `exchange` over BlueZ D-Bus).
use super::{client::Bluez, objects, Error};
use crate::Address;
use dbus::{
    arg::{PropMap, RefArg, Variant},
    Message,
};
use std::{collections::HashMap, time::Duration};

pub const GATT: &str = "org.bluez.GattCharacteristic1";
/// The Carrot-B1 control service.
pub const B1_SERVICE: &str = "db340a90-804c-4ead-af37-8a0e6af754a1";
const SERVICE: &str = "org.bluez.GattService1";

/// A device as discovery sees it, with its characteristics in `service`.
pub struct Discovered {
    pub connected: bool,
    pub rssi: Option<i64>,
    pub name: Option<String>,
    pub uuids: Vec<String>,
    /// Characteristic UUID to object path.
    pub characteristics: HashMap<String, String>,
}

fn text(props: Option<&PropMap>, key: &str) -> Option<String> {
    props?.get(key)?.0.as_str().map(str::to_owned)
}

impl Bluez {
    /// `None` when BlueZ has no object for `address`.
    pub async fn discover_gatt(
        &mut self,
        address: &Address,
        service: &str,
    ) -> Result<Option<Discovered>, Error> {
        let objects = self.objects_public().await?;
        let Some((path, interfaces)) = objects.iter().find(|(_, interfaces)| {
            objects::address(interfaces)
                .ok()
                .flatten()
                .is_some_and(|found| found.to_uppercase() == address.as_str())
        }) else {
            return Ok(None);
        };
        let device = interfaces.get(super::DEVICE);
        let uuids = device
            .and_then(|props| props.get("UUIDs"))
            .and_then(|value| value.0.as_iter())
            .map(|values| {
                values
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let mut characteristics = HashMap::new();
        for (attr, interfaces) in &objects {
            let Some(char_props) = interfaces.get(GATT) else {
                continue;
            };
            let Some(service_path) = text(Some(char_props), "Service") else {
                continue;
            };
            let parent = objects
                .iter()
                .find(|(p, _)| p.to_string() == service_path)
                .and_then(|(_, i)| i.get(SERVICE));
            if text(parent, "Device").as_deref() == Some(&path.to_string())
                && text(parent, "UUID").as_deref() == Some(service)
            {
                if let Some(uuid) = text(Some(char_props), "UUID") {
                    characteristics.insert(uuid, attr.to_string());
                }
            }
        }
        Ok(Some(Discovered {
            connected: device
                .and_then(|p| p.get("Connected"))
                .and_then(|v| v.0.as_i64())
                .is_some_and(|v| v != 0),
            rssi: device
                .and_then(|p| p.get("RSSI"))
                .and_then(|v| v.0.as_i64()),
            name: text(device, "Name"),
            uuids,
            characteristics,
        }))
    }

    pub async fn read_value(&mut self, path: &str, timeout: Duration) -> Result<Vec<u8>, Error> {
        let peer = self.ensure().await?;
        let message = Message::new_method_call("org.bluez", path, GATT, "ReadValue")
            .map_err(Error::Request)?
            .append1(PropMap::new());
        Ok(peer.send(message, timeout).await?.read1()?)
    }

    pub async fn write_value(
        &mut self,
        path: &str,
        value: &[u8],
        timeout: Duration,
    ) -> Result<(), Error> {
        let peer = self.ensure().await?;
        let mut options = PropMap::new();
        options.insert(
            "type".into(),
            Variant(Box::new("request".to_owned()) as Box<dyn RefArg>),
        );
        let message = Message::new_method_call("org.bluez", path, GATT, "WriteValue")
            .map_err(Error::Request)?
            .append2(value.to_vec(), options);
        peer.send(message, timeout).await?;
        Ok(())
    }
}
