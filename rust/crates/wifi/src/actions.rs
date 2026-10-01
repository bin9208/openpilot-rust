use crate::{
    bus::{self, Address},
    engine::Engine,
    settings, Command, Error, Event, MeteredType,
};

impl Engine {
    pub async fn execute(&self, command: Command) -> Result<(), Error> {
        match command {
            Command::SetActive(false) | Command::SetIpv4Forward(_) | Command::Stop => Ok(()),
            Command::SetActive(true) => {
                let (state, networks) =
                    tokio::join!(self.init_wifi_state(), self.update_networks());
                state?;
                networks
            }
            Command::Connect {
                ssid,
                password,
                hidden,
            } => self.connect(&ssid, &password, hidden).await,
            Command::Forget(ssid) => self.forget(&ssid).await,
            Command::Activate(ssid) => self.activate(&ssid).await,
            Command::SetTetheringPassword(password) => self.set_password(&password).await,
            Command::SetTetheringActive(active) => self.tether(active).await,
            Command::SetCurrentNetworkMetered(metered) => self.metered(metered).await,
        }
    }
    async fn connect(&self, ssid: &str, password: &str, hidden: bool) -> Result<(), Error> {
        self.forget(ssid).await?;
        let settings = settings::connection(ssid, password, hidden);
        let device = self.state()?.device.clone();
        let Some(device) = device else {
            self.warn("No WiFi device found".into());
            return self.init_wifi_state().await;
        };
        let options =
            dbus::arg::PropMap::from([("persist".into(), settings::value("volatile".to_owned()))]);
        let reply: Result<(dbus::Path<'static>, dbus::Path<'static>, dbus::arg::PropMap), Error> =
            self.main
                .call(
                    Address {
                        path: bus::NM_PATH,
                        interface: bus::NM,
                    },
                    "AddAndActivateConnection2",
                    (
                        settings,
                        dbus::Path::new(device).map_err(Error::Request)?,
                        dbus::Path::from("/"),
                        options,
                    ),
                )
                .await;
        match reply {
            Ok(_) => Ok(()),
            Err(Error::Reply(error)) => {
                self.warn(format!(
                    "Failed to add and activate connection for {ssid}: {error}"
                ));
                self.init_wifi_state().await
            }
            Err(error) => Err(error),
        }
    }
    pub async fn forget(&self, ssid: &str) -> Result<(), Error> {
        let path = self.state()?.connection(ssid).map(str::to_owned);
        if let Some(path) = path {
            match self
                .main
                .call::<_, ()>(
                    Address {
                        path: &path,
                        interface: bus::CONNECTION,
                    },
                    "Delete",
                    (),
                )
                .await
            {
                Ok(()) | Err(Error::Reply(_)) => {}
                Err(error) => return Err(error),
            }
        } else {
            self.warn(format!("Trying to forget unknown connection: {ssid}"));
        }
        self.state()?.events.push(Event::Forgotten(ssid.to_owned()));
        Ok(())
    }
    pub async fn activate(&self, ssid: &str) -> Result<(), Error> {
        let (connection, device) = {
            let state = self.state()?;
            (
                state.connection(ssid).map(str::to_owned),
                state.device.clone(),
            )
        };
        let (Some(connection), Some(device)) = (connection, device) else {
            self.warn(format!(
                "Failed to activate connection for {ssid}: connection or WiFi device unavailable"
            ));
            return self.init_wifi_state().await;
        };
        let reply: Result<(dbus::Path<'static>,), Error> = self
            .main
            .call(
                Address {
                    path: bus::NM_PATH,
                    interface: bus::NM,
                },
                "ActivateConnection",
                (
                    dbus::Path::new(connection).map_err(Error::Request)?,
                    dbus::Path::new(device).map_err(Error::Request)?,
                    dbus::Path::from("/"),
                ),
            )
            .await;
        match reply {
            Ok(_) => Ok(()),
            Err(Error::Reply(error)) => {
                self.warn(format!("Failed to activate connection for {ssid}: {error}"));
                self.init_wifi_state().await
            }
            Err(error) => Err(error),
        }
    }
    async fn metered(&self, metered: MeteredType) -> Result<(), Error> {
        if self.state()?.snapshot().tethering_active {
            return Ok(());
        }
        let Some((path, _)) = self.active_connection(&self.main).await? else {
            self.warn("No active WiFi connection found".into());
            return Ok(());
        };
        let mut settings = self.connection_settings(&path).await?;
        if settings.is_empty() {
            self.warn(format!("Failed to get connection settings for {path}"));
            return Ok(());
        }
        let code = match metered {
            MeteredType::Unknown => 0_i32,
            MeteredType::Yes => 1,
            MeteredType::No => 2,
        };
        settings
            .get_mut("connection")
            .ok_or(Error::Property("connection"))?
            .insert("metered".into(), settings::value(code));
        match self
            .main
            .call::<_, ()>(
                Address {
                    path: &path,
                    interface: bus::CONNECTION,
                },
                "Update",
                (settings,),
            )
            .await
        {
            Ok(()) => Ok(()),
            Err(Error::Reply(error)) => {
                self.warn(format!("Failed to update metered settings: {error}"));
                Ok(())
            }
            Err(error) => Err(error),
        }
    }
}
