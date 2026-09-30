use crate::{
    at::{AtClient, Config, ISDR_AID},
    codec::{encode, to_tbcd},
    http::{Es9, Http},
    notifications, protocol as p, Error, Result,
};
use rustix::fs::{flock, FlockOperation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::{File, OpenOptions},
    os::unix::fs::OpenOptionsExt,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Profile {
    pub iccid: Option<String>,
    pub nickname: String,
    pub enabled: bool,
    pub provider: String,
}
impl Profile {
    pub fn is_comma(&self) -> bool {
        self.provider == "Webbing"
            && self
                .iccid
                .as_ref()
                .is_some_and(|s| s.starts_with("8985235"))
    }
}
pub struct Lpa {
    pub client: AtClient,
}
impl Lpa {
    pub fn new(config: Config) -> Self {
        Self {
            client: AtClient::new(config),
        }
    }
    fn lock(&self) -> Result<File> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o777)
            .open(&self.client.config.lock)?;
        flock(&file, FlockOperation::LockExclusive).map_err(std::io::Error::from)?;
        Ok(file)
    }
    pub fn with_channel<T>(
        &mut self,
        operation: impl FnOnce(&mut AtClient) -> Result<T>,
    ) -> Result<T> {
        let _lock = self.lock()?;
        let result = self
            .client
            .open_isdr()
            .and_then(|()| operation(&mut self.client));
        self.client.close_channel()?;
        result
    }
    pub fn list_profiles(&mut self) -> Result<Vec<Profile>> {
        self.with_channel(|client| {
            p::list_profiles(client).map(|profiles| {
                profiles
                    .into_iter()
                    .map(|p| Profile {
                        iccid: p["iccid"].as_str().map(str::to_owned),
                        nickname: p["profileNickname"].as_str().unwrap_or("").into(),
                        enabled: p["profileState"] == "enabled",
                        provider: p["serviceProviderName"].as_str().unwrap_or("").into(),
                    })
                    .collect()
            })
        })
    }
    pub fn get_active_profile(&self) -> Option<Profile> {
        None
    }
    pub fn delete_profile(&mut self, iccid: &str) -> Result<()> {
        let profile = self
            .list_profiles()?
            .into_iter()
            .find(|p| p.iccid.as_deref() == Some(iccid))
            .ok_or_else(|| Error::ProfileNotFound(iccid.into()))?;
        if profile.is_comma() {
            return Err(Error::Lpa("refusing to delete a comma profile".into()));
        }
        let code = self.with_channel(|client| {
            p::status(
                &p::command(client, &encode(0xbf33, &encode(0x5a, &to_tbcd(iccid))))?,
                0xbf33,
                "DeleteProfileResponse",
                "DeleteProfile status",
            )
        })?;
        if code != 0 {
            return Err(Error::Lpa(format!(
                "DeleteProfile failed: {} (0x{code:02X})",
                p::profile_error(code)
            )));
        }
        Ok(())
    }
    pub fn nickname_profile(&mut self, iccid: &str, nickname: &str) -> Result<()> {
        self.with_channel(|client| p::nickname(client, iccid, nickname))
    }
    pub fn switch_profile(&mut self, iccid: &str) -> Result<()> {
        self.with_channel(|client| {
            let mut code = p::enable(client, iccid)?;
            if code == 5 {
                client.reset()?;
                client.open_isdr()?;
                code = p::enable(client, iccid)?;
            }
            if !matches!(code, 0 | 2) {
                return Err(Error::Lpa(format!(
                    "EnableProfile failed: {} (0x{code:02X})",
                    p::profile_error(code)
                )));
            }
            Ok(())
        })
    }
    pub fn download_profile(&mut self, activation: &str, nickname: Option<&str>) -> Result<()> {
        self.download_with_http(&mut Http::new()?, activation, nickname)
    }
    pub fn download_with_http(
        &mut self,
        http: &mut impl Es9,
        activation: &str,
        nickname: Option<&str>,
    ) -> Result<()> {
        self.with_channel(|client| {
            let iccid = p::download(client, http, activation)?;
            if let (Some(nickname), Some(iccid)) = (
                nickname.filter(|s| !s.is_empty()),
                iccid.filter(|s| !s.is_empty()),
            ) {
                p::nickname(client, &iccid, nickname)?;
            }
            Ok(())
        })
    }
    pub fn process_notifications(&mut self) -> Result<()> {
        struct FreshHttp;
        impl Es9 for FreshHttp {
            fn request(
                &mut self,
                address: &str,
                endpoint: &str,
                payload: Value,
                prefix: &str,
            ) -> Result<Value> {
                Http::new()?.request(address, endpoint, payload, prefix)
            }
        }
        self.process_notifications_with_http(&mut FreshHttp)
    }
    pub fn process_notifications_with_http(&mut self, http: &mut impl Es9) -> Result<()> {
        crate::http::require_time()?;
        self.with_channel(|client| notifications::process(client, http))
    }
    pub fn is_euicc(&mut self) -> Result<bool> {
        let _lock = self.lock()?;
        let lines = match self.client.query(&format!("AT+CCHO=\"{ISDR_AID}\"")) {
            Err(Error::Protocol(_)) => return Ok(false),
            other => other?,
        };
        for line in lines {
            if let Some(ch) = line
                .strip_prefix("+CCHO:")
                .map(str::trim)
                .filter(|ch| !ch.is_empty())
            {
                match self.client.query(&format!("AT+CCHC={ch}")) {
                    Err(Error::Protocol(_) | Error::Timeout) => {}
                    other => {
                        other?;
                    }
                }
                self.client.channel = None;
                return Ok(true);
            }
        }
        Ok(false)
    }
}
