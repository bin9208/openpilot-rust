use crate::{
    bus::{self, Address},
    engine::Engine,
    settings, Error,
};
use openpilot_logging::record::Level;
use openpilot_process_supervision::{CapturedChild, CapturedCommand};
use std::time::Duration;

struct CommandChild(CapturedChild);
impl Drop for CommandChild {
    fn drop(&mut self) {
        match self.0.process.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => {}
            Err(error) => eprintln!("Wi-Fi child status: {error}"),
        }
        if let Err(error) = self.0.process.kill() {
            eprintln!("Wi-Fi child kill: {error}");
        }
        if let Err(error) = self.0.process.wait() {
            eprintln!("Wi-Fi child reap: {error}");
        }
    }
}
impl Engine {
    pub async fn set_password(&self, password: &str) -> Result<(), Error> {
        let path = {
            let state = self.state()?;
            state.connection(&state.tethering_ssid).map(str::to_owned)
        };
        let Some(path) = path else {
            self.warn("No tethering connection found".into());
            return Ok(());
        };
        let mut settings = self.connection_settings(&path).await?;
        if settings.is_empty() {
            self.warn(format!("Failed to get tethering settings for {path}"));
            return Ok(());
        }
        settings
            .get_mut("802-11-wireless-security")
            .ok_or(Error::Property("802-11-wireless-security"))?
            .insert("psk".into(), settings::value(password.to_owned()));
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
            Ok(()) => {}
            Err(Error::Reply(error)) => {
                self.warn(format!("Failed to update tethering settings: {error}"));
                return Ok(());
            }
            Err(error) => return Err(error),
        }
        let reconnect = {
            let mut state = self.state()?;
            state.snapshot.tethering_password = password.to_owned();
            state
                .snapshot()
                .tethering_active
                .then(|| state.tethering_ssid.clone())
        };
        if let Some(ssid) = reconnect {
            self.state()?.set_connecting(Some(ssid.clone()));
            self.activate(&ssid).await?;
        }
        Ok(())
    }
    pub async fn password(&self) -> Result<String, Error> {
        let path = {
            let state = self.state()?;
            state.connection(&state.tethering_ssid).map(str::to_owned)
        };
        let Some(path) = path else {
            self.warn("No tethering connection found".into());
            return Ok(String::new());
        };
        let reply: Result<(bus::Settings,), Error> = self
            .main
            .call(
                Address {
                    path: &path,
                    interface: bus::CONNECTION,
                },
                "GetSecrets",
                ("802-11-wireless-security",),
            )
            .await;
        match reply {
            Ok((secrets,)) => secrets
                .get("802-11-wireless-security")
                .and_then(|group| group.get("psk"))
                .map(|value| bus::text(value, "psk"))
                .transpose()
                .map(Option::unwrap_or_default),
            Err(Error::Reply(error)) => {
                self.warn(format!("Failed to get tethering password: {error}"));
                Ok(String::new())
            }
            Err(error) => Err(error),
        }
    }
    pub async fn tether(&self, enabled: bool) -> Result<(), Error> {
        let ssid = self.state()?.tethering_ssid.clone();
        if enabled {
            self.state()?.set_connecting(Some(ssid.clone()));
            self.activate(&ssid).await?;
            if !self.state()?.ipv4_forward {
                tokio::time::sleep(Duration::from_secs(5)).await;
                self.log(Level::Warning, "net.ipv4.ip_forward = 0".into());
                let command = CapturedCommand {
                    launcher: self.launcher.clone(),
                    cwd: std::env::current_dir()?,
                    argv: ["sudo", "sysctl", "net.ipv4.ip_forward=0"]
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                };
                let mut child = tokio::task::spawn_blocking(move || {
                    command.spawn_inherited().map(CommandChild)
                })
                .await
                .map_err(|_| Error::Panicked)??;
                while child.0.process.try_wait()?.is_none() {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }
        } else {
            self.deactivate(&ssid).await?;
        }
        Ok(())
    }
    async fn deactivate(&self, ssid: &str) -> Result<(), Error> {
        let paths = self
            .main
            .get(
                Address {
                    path: bus::NM_PATH,
                    interface: bus::NM,
                },
                "ActiveConnections",
            )
            .await?;
        for path in bus::paths(&paths, "ActiveConnections")? {
            let object = match self
                .main
                .get(
                    Address {
                        path: &path,
                        interface: bus::ACTIVE,
                    },
                    "SpecificObject",
                )
                .await
            {
                Ok(value) => bus::text(&value, "SpecificObject")?,
                Err(Error::Reply(_)) => continue,
                Err(error) => return Err(error),
            };
            if object == "/" {
                continue;
            }
            let value = match self
                .main
                .get(
                    Address {
                        path: &object,
                        interface: bus::ACCESS_POINT,
                    },
                    "Ssid",
                )
                .await
            {
                Ok(value) => value,
                Err(Error::Reply(_)) => continue,
                Err(error) => return Err(error),
            };
            if String::from_utf8_lossy(&bus::bytes(&value, "Ssid")?) == ssid {
                match self
                    .main
                    .call::<_, ()>(
                        Address {
                            path: bus::NM_PATH,
                            interface: bus::NM,
                        },
                        "DeactivateConnection",
                        (dbus::Path::new(path).map_err(Error::Request)?,),
                    )
                    .await
                {
                    Ok(()) | Err(Error::Reply(_)) => return Ok(()),
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(())
    }
}
