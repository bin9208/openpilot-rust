use super::{client::Bluez, peer::Endpoint, state::Pair, Error, AGENT, DEVICE};
use crate::Address;
use dbus::arg::Variant;
use openpilot_logmessaged::JsonValue;

impl Bluez {
    pub async fn start_pair(&mut self, address: &Address) -> Result<(), Error> {
        if let Some(done) = self.pair.try_join_next() {
            done??;
        }
        if !self.pair.is_empty() {
            return Err(Error::Pairing);
        }
        let path = self.locate(address).await?;
        let peer = self.ensure().await?;
        if !self.registered {
            let owner = peer.owner().await?;
            self.shared.lock()?.owner = Some(owner);
            let agent = dbus::Path::new(AGENT).map_err(Error::Request)?;
            peer.call::<_, ()>(
                Endpoint {
                    path: "/org/bluez",
                    interface: "org.bluez.AgentManager1",
                    method: "RegisterAgent",
                    seconds: 10,
                },
                (agent, "KeyboardDisplay"),
            )
            .await?;
            self.registered = true;
        }
        {
            let mut state = self.shared.lock()?;
            state.target = Some(path.to_string());
            state.pair = Pair::Pairing {
                address: address.as_str().to_owned(),
            };
        }
        let shared = self.shared.clone();
        let address = address.as_str().to_owned();
        self.pair.spawn(async move {
            let result = async {
                peer.call::<_, ()>(
                    Endpoint {
                        path: &path,
                        interface: DEVICE,
                        method: "Pair",
                        seconds: 90,
                    },
                    (),
                )
                .await?;
                peer.call::<_, ()>(
                    Endpoint {
                        path: &path,
                        interface: "org.freedesktop.DBus.Properties",
                        method: "Set",
                        seconds: 10,
                    },
                    (DEVICE, "Trusted", Variant(true)),
                )
                .await?;
                shared.lock()?.pair = Pair::Paired {
                    address: address.clone(),
                    error: None,
                };
                if let Err(error) = peer
                    .call::<_, ()>(
                        Endpoint {
                            path: &path,
                            interface: DEVICE,
                            method: "Connect",
                            seconds: 20,
                        },
                        (),
                    )
                    .await
                {
                    shared.lock()?.pair = Pair::Paired {
                        address: address.clone(),
                        error: Some(error.to_string()),
                    };
                }
                Ok::<(), Error>(())
            }
            .await;
            if let Err(error) = result {
                shared.lock()?.pair = Pair::Error {
                    address,
                    error: error.to_string(),
                };
            }
            shared.finish()
        });
        Ok(())
    }

    pub fn respond(&self, id: &str, value: JsonValue) -> Result<(), Error> {
        self.shared.respond(id, value)
    }

    pub async fn cancel_pair(&mut self) -> Result<(), Error> {
        let target = self.shared.lock()?.target.clone();
        if let (Some(path), Some(connection)) = (target, &self.connection) {
            if let Err(error) = connection
                .peer
                .call::<_, ()>(
                    Endpoint {
                        path: &path,
                        interface: DEVICE,
                        method: "CancelPairing",
                        seconds: 10,
                    },
                    (),
                )
                .await
            {
                eprintln!("Bluetooth CancelPairing: {error}");
            }
        }
        if let Some(done) = self.pair.try_join_next() {
            done??;
        }
        if !self.pair.is_empty() {
            self.pair.abort_all();
            while let Some(done) = self.pair.join_next().await {
                match done {
                    Err(error) if error.is_cancelled() => {
                        let mut state = self.shared.lock()?;
                        let address = match &state.pair {
                            Pair::Pairing { address }
                            | Pair::Paired { address, .. }
                            | Pair::Error { address, .. }
                            | Pair::Cancelled { address } => address.clone(),
                            Pair::Idle => return Err(Error::Property("pair address")),
                        };
                        state.pair = Pair::Cancelled { address };
                    }
                    Ok(result) => result?,
                    Err(error) => return Err(error.into()),
                }
            }
            self.shared.finish()?;
        }
        Ok(())
    }
}
