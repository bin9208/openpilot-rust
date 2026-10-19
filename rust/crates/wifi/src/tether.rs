use crate::{
    bus::{self, Address},
    engine::Engine,
    settings, Error,
};
use openpilot_logging::record::Level;
use openpilot_process_supervision::{CapturedChild, CapturedCommand};
use std::time::Duration;

struct CommandChild(CapturedChild);
impl CommandChild {
    fn spawn(
        command: CapturedCommand,
        environment: &[(std::ffi::OsString, std::ffi::OsString)],
    ) -> Result<Self, openpilot_process_supervision::Error> {
        command
            .spawn_session_inherited_with_env(environment)
            .map(Self)
    }

    fn exited(&self) -> std::io::Result<bool> {
        use rustix::process::{waitid, Pid, WaitId, WaitIdOptions};
        loop {
            match waitid(
                WaitId::Pid(Pid::from_child(&self.0.process)),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            ) {
                Ok(status) => return Ok(status.is_some()),
                Err(rustix::io::Errno::INTR) => continue,
                Err(error) => return Err(error.into()),
            }
        }
    }
}
impl Drop for CommandChild {
    fn drop(&mut self) {
        // The session leader is retained, even after normal exit, until every
        // signal using its process-group identity has finished.
        use rustix::process::{kill_process_group, Pid, Signal};
        match kill_process_group(Pid::from_child(&self.0.process), Signal::KILL) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => {}
            Err(error) => {
                eprintln!("Wi-Fi child group kill: {error}");
                if let Err(error) = self.0.process.kill() {
                    eprintln!("Wi-Fi child kill: {error}");
                }
            }
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
                let child = tokio::task::spawn_blocking(move || CommandChild::spawn(command, &[]))
                    .await
                    .map_err(|_| Error::Panicked)??;
                while !child.exited()? {
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

#[cfg(all(test, target_os = "linux"))]
mod cleanup_tests;
