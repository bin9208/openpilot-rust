use crate::{
    bus::{self, Address, Peer, Settings},
    state::State,
    Error,
};
use openpilot_logging::{
    log_site,
    producer::Logger,
    record::{Level, Record},
};
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, Mutex, MutexGuard},
};

#[derive(Clone)]
pub struct Engine {
    pub state: Arc<Mutex<State>>,
    pub main: Peer,
    pub monitor: Peer,
    pub logger: Rc<RefCell<Logger>>,
    pub scan_lock: Rc<tokio::sync::Mutex<()>>,
    pub launcher: PathBuf,
}
impl Engine {
    pub fn state(&self) -> Result<MutexGuard<'_, State>, Error> {
        self.state.lock().map_err(|_| Error::Poisoned)
    }
    pub fn log(&self, level: Level, message: String) {
        if let Err(error) = self
            .logger
            .borrow_mut()
            .emit(log_site!(), Record::text(level, message))
        {
            eprintln!("Wi-Fi logging failed: {error}");
        }
    }
    pub fn warn(&self, message: String) {
        self.log(Level::Warning, message);
    }
    pub async fn connection_settings(&self, path: &str) -> Result<Settings, Error> {
        match self
            .main
            .call(
                Address {
                    path,
                    interface: bus::CONNECTION,
                },
                "GetSettings",
                (),
            )
            .await
        {
            Ok((settings,)) => Ok(settings),
            Err(Error::Reply(error)) => {
                self.warn(format!("Failed to get connection settings: {error}"));
                Ok(Settings::new())
            }
            Err(error) => Err(error),
        }
    }
    pub async fn active_connection(
        &self,
        peer: &Peer,
    ) -> Result<Option<(String, dbus::arg::PropMap)>, Error> {
        let paths = peer
            .get(
                Address {
                    path: bus::NM_PATH,
                    interface: bus::NM,
                },
                "ActiveConnections",
            )
            .await?;
        for path in bus::paths(&paths, "ActiveConnections")? {
            let props = match peer
                .properties(Address {
                    path: &path,
                    interface: bus::ACTIVE,
                })
                .await
            {
                Ok(props) => props,
                Err(Error::Reply(error)) => {
                    self.warn(format!(
                        "Failed to get active connection properties for {path}: {error}"
                    ));
                    continue;
                }
                Err(error) => return Err(error),
            };
            let connection = props
                .get("Connection")
                .map(|v| bus::text(v, "Connection"))
                .transpose()?
                .unwrap_or_else(|| "/".into());
            let kind = props
                .get("Type")
                .map(|v| bus::text(v, "Type"))
                .transpose()?
                .unwrap_or_default();
            if kind == "802-11-wireless" && connection != "/" {
                return Ok(Some((connection, props)));
            }
        }
        Ok(None)
    }
    pub async fn init_wifi_state(&self) -> Result<(), Error> {
        let (device, epoch) = {
            let state = self.state()?;
            (state.device.clone(), state.epoch)
        };
        let Some(device) = device else {
            self.warn("No WiFi device found".into());
            return Ok(());
        };
        let value = self
            .main
            .get(
                Address {
                    path: &device,
                    interface: bus::DEVICE,
                },
                "State",
            )
            .await?;
        let device_state = bus::unsigned(&value, "State")?;
        let active = self.active_connection(&self.main).await?;
        self.state()?.finish_initial_state(
            epoch,
            device_state,
            active.as_ref().map(|(path, _)| path.as_str()),
        );
        Ok(())
    }
    pub async fn refresh_connection(&self, path: String) -> Result<(), Error> {
        let settings = self.connection_settings(&path).await?;
        if let Some(wifi) = settings.get("802-11-wireless") {
            let ssid = wifi.get("ssid").ok_or(Error::Property("ssid"))?;
            let ssid = String::from_utf8_lossy(&bus::bytes(ssid, "ssid")?).into_owned();
            self.state()?.new_connection(ssid, path);
        }
        Ok(())
    }
}
