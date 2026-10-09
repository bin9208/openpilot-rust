use super::{bad, state::atomic, Failure, Service, Value};
use crate::json_fields::set;
use hyper::Request;
use openpilot_bluetooth::{bluez::Action, Address, Config};

fn address(body: &Value) -> Result<Address, Failure> {
    let text = body
        .get("address")
        .string()
        .map_err(|_| bad("invalid Bluetooth address"))?;
    Ok(Address::parse(&text)?)
}

impl Service {
    pub(super) async fn mutate<T>(
        &self,
        request: &Request<T>,
        operation: &str,
        body: Value,
    ) -> Result<(), Failure> {
        if !matches!(body, Value::Object(_)) {
            return Err(bad("object required"));
        }
        let _mutation = self.mutation.lock().await;
        super::http::guard(request, self)?;
        let mut client = self.client.lock().await;
        match operation {
            "scan" => {
                self.cache(Some(client.snapshot_reader().await?))?;
                client.scan().await?;
            }
            "pair" => {
                let mac = address(&body)?;
                self.cache(Some(client.snapshot_reader().await?))?;
                client.start_pair(&mac).await?;
            }
            "cancel" => client.cancel_pair().await?,
            "answer" => {
                let id = if let Value::Text(_) = body.get("id") {
                    body.get("id").string().unwrap_or_default()
                } else {
                    String::new()
                };
                client.respond(
                    &id,
                    openpilot_logmessaged::JsonValue::parse(&body.get("value").encode()?)?,
                )?;
            }
            "connect" | "disconnect" | "forget" => {
                let mac = address(&body)?;
                self.cache(Some(client.snapshot_reader().await?))?;
                let action = match operation {
                    "connect" => Action::Connect,
                    "disconnect" => Action::Disconnect,
                    _ => Action::Forget,
                };
                client.device_action(&mac, action).await?;
                if operation != "connect" {
                    self.cancel_pending(&mac)?;
                }
                if operation == "forget" {
                    let mut settings = self.settings();
                    settings.devices.shift_remove(&mac);
                    openpilot_bluetooth::atomic_json(&self.config, &settings)?;
                }
            }
            "config" | "device-config" => {
                let previous = self.settings();
                let mut proposed = body.clone();
                if operation == "device-config" {
                    let mac = address(&body)?;
                    proposed = super::value(&self.settings())?;
                    let Value::Object(fields) = &mut proposed else {
                        return Err(bad("devices must be an object"));
                    };
                    let devices = fields
                        .iter_mut()
                        .find(|(name, _)| name.iter().copied().eq("devices".chars().map(u32::from)))
                        .ok_or_else(|| bad("devices must be an object"))?;
                    set(&mut devices.1, mac.as_str(), body.get("device").clone())?;
                }
                let settings = Config::parse(&proposed.encode()?)?;
                self.cache(Some(client.snapshot_reader().await?))?;
                let snapshot = client.snapshot().await?;
                if settings.devices.keys().any(|mac| {
                    !snapshot.devices.iter().any(|device| {
                        device.paired && device.address.as_deref() == Some(mac.as_str())
                    })
                }) {
                    return Err(bad("pair devices before configuring input"));
                }
                openpilot_bluetooth::atomic_json(&self.config, &settings)?;
                for (mac, old) in previous.devices {
                    if settings.devices.get(&mac) != Some(&old) {
                        self.cancel_pending(&mac)?;
                    }
                }
            }
            "learn" => {
                let mac = address(&body)?;
                if !self.settings().devices.contains_key(&mac) {
                    return Err(bad("save the input profile first"));
                }
                let Value::Bool(enabled) = body.get("enabled") else {
                    return Err(bad("enabled must be boolean"));
                };
                let value = if *enabled {
                    Value::object([
                        ("address", Value::text(mac.as_str())),
                        ("until", Value::Float(self.now() + 120.)),
                    ])
                } else {
                    Value::Object(Vec::new())
                };
                atomic(&self.runtime.join("learn.json"), &value)?;
                self.cancel_pending(&mac)?;
            }
            "radio" => {
                let Value::Bool(enabled) = body.get("enabled") else {
                    return Err(bad("enabled must be boolean"));
                };
                self.cache(None)?;
                client.close().await?;
                drop(client);
                self.radio(*enabled).await?;
            }
            _ => {
                return Err(Failure::Http(
                    hyper::StatusCode::NOT_FOUND,
                    "404: Not Found".into(),
                ))
            }
        }
        Ok(())
    }
}
