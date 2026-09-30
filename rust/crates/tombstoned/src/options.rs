use crate::Error;
use std::{
    env,
    ffi::OsString,
    num::NonZeroU64,
    path::{Path, PathBuf},
};

pub struct Options {
    pub apport: PathBuf,
    pub log_root: PathBuf,
    pub base: PathBuf,
    pub cycles: Option<NonZeroU64>,
    pub local_dsn: Option<String>,
    pub local_device: Option<String>,
}
fn log_root() -> Result<PathBuf, Error> {
    if let Some(root) = env::var_os("LOG_ROOT").filter(|root| !root.is_empty()) {
        return Ok(root.into());
    }
    if Path::new("/TICI").is_file() {
        return Ok("/data/media/0/realdata/".into());
    }
    let mut name = OsString::from(".comma");
    name.push(env::var_os("OPENPILOT_PREFIX").unwrap_or_default());
    Ok(home::home_dir()
        .ok_or(Error::Contract("home directory unavailable"))?
        .join(name)
        .join("media/0/realdata"))
}
impl Options {
    pub fn parse(arguments: impl Iterator<Item = OsString>) -> Result<Option<Self>, Error> {
        let mut args = arguments;
        let mut apport = None;
        let mut root = None;
        let mut base = None;
        let mut cycles = None;
        let mut local_dsn = None;
        let mut local_device = None;
        while let Some(argument) = args.next() {
            if argument == "--help" {
                println!("openpilot-tombstoned [--cycles N] [--base-dir DIR] [--apport-dir DIR] [--log-root DIR]\n[--local-sentry-dsn DSN --local-reporting-device DEVICE]\n\nNative crash-file daemon. Default crash input is /var/crash; runtime selection is unchanged.\nHost validation must use an isolated apport directory. Local reporting hardware simulation\nrequires a loopback-only Sentry capture DSN; it does not bypass origin/registration gates.");
                return Ok(None);
            }
            let value = args.next().ok_or(Error::Contract("missing option value"))?;
            match argument.to_str() {
                Some("--apport-dir") if apport.is_none() => apport = Some(PathBuf::from(value)),
                Some("--log-root") if root.is_none() => root = Some(PathBuf::from(value)),
                Some("--base-dir") if base.is_none() => base = Some(PathBuf::from(value)),
                Some("--cycles") if cycles.is_none() => {
                    cycles = Some(
                        value
                            .to_str()
                            .and_then(|text| text.parse().ok())
                            .ok_or(Error::Contract("cycles must be positive"))?,
                    )
                }
                Some("--local-sentry-dsn") if local_dsn.is_none() => {
                    local_dsn = Some(
                        value
                            .into_string()
                            .map_err(|_| Error::Contract("DSN is not UTF-8"))?,
                    )
                }
                Some("--local-reporting-device") if local_device.is_none() => {
                    local_device = Some(
                        value
                            .into_string()
                            .map_err(|_| Error::Contract("device is not UTF-8"))?,
                    )
                }
                _ => return Err(Error::Contract("unknown or duplicate option; see --help")),
            }
        }
        if local_device.is_some() && local_dsn.is_none() {
            return Err(Error::Contract(
                "local device simulation requires local capture DSN",
            ));
        }
        Ok(Some(Self {
            apport: apport.unwrap_or_else(|| "/var/crash/".into()),
            log_root: match root {
                Some(root) => root,
                None => log_root()?,
            },
            base: match base {
                Some(base) => base,
                None => env::current_dir()?,
            },
            cycles,
            local_dsn,
            local_device,
        }))
    }
}
