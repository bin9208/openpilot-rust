use crate::{
    bus::{self, Address},
    engine::Engine,
    settings,
    state::State,
    Error,
};
use openpilot_logging::record::Level;
use std::time::Duration;
use tokio::task::JoinSet;

impl Engine {
    async fn adapter(&self) -> Result<Option<String>, Error> {
        let (paths,): (Vec<dbus::Path<'static>>,) = self
            .main
            .call(
                Address {
                    path: bus::NM_PATH,
                    interface: bus::NM,
                },
                "GetDevices",
                (),
            )
            .await?;
        for path in paths {
            let value = self
                .main
                .get(
                    Address {
                        path: &path,
                        interface: bus::DEVICE,
                    },
                    "DeviceType",
                )
                .await?;
            if bus::unsigned(&value, "DeviceType")? == 2 {
                return Ok(Some(path.to_string()));
            }
        }
        Ok(None)
    }
    pub async fn initialize(&self) -> Result<(), Error> {
        loop {
            match self.adapter().await {
                Ok(Some(device)) => {
                    self.state()?.device = Some(device);
                    break;
                }
                Ok(None) => {}
                Err(error) => self.log(
                    Level::Error,
                    format!("Error getting adapter type 2: {error}"),
                ),
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        let mut workers = JoinSet::new();
        let scanner = self.clone();
        workers.spawn_local(async move { scanner.scan().await });
        let monitor = self.clone();
        workers.spawn_local(async move { monitor.monitor().await });
        let setup = async {
            let (paths,): (Vec<dbus::Path<'static>>,) = self
                .main
                .call(
                    Address {
                        path: bus::SETTINGS_PATH,
                        interface: bus::SETTINGS,
                    },
                    "ListConnections",
                    (),
                )
                .await?;
            let mut known = State::new(None);
            for path in paths {
                let settings = self.connection_settings(&path).await?;
                if settings.is_empty() {
                    self.warn(format!("Failed to get connection settings for {path}"));
                    continue;
                }
                if let Some(wifi) = settings.get("802-11-wireless") {
                    let ssid = wifi.get("ssid").ok_or(Error::Property("ssid"))?;
                    known.new_connection(
                        String::from_utf8_lossy(&bus::bytes(ssid, "ssid")?).into_owned(),
                        path.to_string(),
                    );
                }
            }
            self.state()?.connections = known.connections;
            let hotspot = {
                let state = self.state()?;
                state
                    .connection(&state.tethering_ssid)
                    .is_none()
                    .then(|| state.tethering_ssid.clone())
            };
            if let Some(ssid) = hotspot {
                match self
                    .main
                    .call::<_, (dbus::Path<'static>,)>(
                        Address {
                            path: bus::SETTINGS_PATH,
                            interface: bus::SETTINGS,
                        },
                        "AddConnection",
                        (settings::hotspot(&ssid),),
                    )
                    .await
                {
                    Ok(_) | Err(Error::Reply(_)) => {}
                    Err(error) => return Err(error),
                }
            }
            self.init_wifi_state().await?;
            let password = self.password().await?;
            self.state()?.snapshot.tethering_password = password;
            self.log(Level::Debug, "WifiManager initialized".into());
            Ok::<(), Error>(())
        }
        .await;
        if let Err(error) = setup {
            self.log(
                Level::Error,
                format!("Wi-Fi initialization stopped: {error}"),
            );
        }
        while let Some(result) = workers.join_next().await {
            match result {
                Ok(Ok(())) => {}
                Ok(Err(error)) => self.log(Level::Error, format!("Wi-Fi worker stopped: {error}")),
                Err(error) => self.log(Level::Error, format!("Wi-Fi worker join: {error}")),
            }
        }
        Ok(())
    }
}
