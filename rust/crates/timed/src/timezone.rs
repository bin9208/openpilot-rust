use crate::{Commands, Error};
use openpilot_logging::{
    log_site,
    producer::Logger,
    record::{Level, Record},
};
use openpilot_params::Params;
use std::{ffi::OsString, io::Read, path::PathBuf, time::Duration};

pub struct Paths {
    pub localtime: PathBuf,
    pub zoneinfo: PathBuf,
    pub systemd: PathBuf,
}
impl Default for Paths {
    fn default() -> Self {
        Self {
            localtime: "/data/etc/localtime".into(),
            zoneinfo: "/usr/share/zoneinfo".into(),
            systemd: "/lib/systemd/systemd".into(),
        }
    }
}
pub struct Services<'a> {
    pub params: &'a Params,
    pub logger: &'a mut Logger,
    pub commands: &'a mut dyn Commands,
    pub paths: &'a Paths,
}
pub fn priority(source: &str) -> u8 {
    match source {
        "app" => 3,
        "wifi" => 2,
        "gps" => 1,
        _ => 0,
    }
}
pub fn current(services: &mut Services<'_>) -> Result<String, Error> {
    Ok(
        openpilot_params_typed::get_string(services.params, "TimezoneSource", services.logger)?
            .unwrap_or_default(),
    )
}
pub fn apply(zone: &str, source: &str, services: &mut Services<'_>) -> Result<bool, Error> {
    let target = services.paths.zoneinfo.join(zone);
    if zone.is_empty() || !target.is_file() {
        services.logger.emit(
            log_site!(),
            Record::text(
                Level::Error,
                format!("timezone: invalid zone '{zone}' (source={source})"),
            ),
        )?;
        return Ok(false);
    }
    if priority(source) < priority(&current(services)?) {
        return Ok(false);
    }
    let localtime = &services.paths.localtime;
    let resolved_target = target.canonicalize()?;
    let already = localtime.is_symlink()
        && std::fs::canonicalize(localtime).is_ok_and(|path| path == resolved_target);
    if !already {
        std::fs::create_dir_all(
            localtime
                .parent()
                .ok_or(Error::Contract("localtime has no parent"))?,
        )?;
        for args in [
            vec![
                OsString::from("rm"),
                OsString::from("-f"),
                localtime.as_os_str().to_owned(),
            ],
            vec![
                OsString::from("ln"),
                OsString::from("-s"),
                target.into_os_string(),
                localtime.as_os_str().to_owned(),
            ],
        ] {
            let status = services.commands.run(&args)?;
            if !status.success() {
                crate::exception(
                    services.logger,
                    "timezone: failed to set /data/etc/localtime",
                    &args,
                    status,
                )?;
                return Ok(false);
            }
        }
    }
    services.params.put("TimezoneName", zone.as_bytes())?;
    services.params.put("TimezoneSource", source.as_bytes())?;
    services.logger.emit(
        log_site!(),
        Record::text(
            Level::Info,
            format!("timezone: set to {zone} (source={source})"),
        ),
    )?;
    Ok(true)
}
pub fn from_gps(longitude: f64) -> Result<String, Error> {
    if !longitude.is_finite() {
        return Err(Error::Contract("nonfinite longitude"));
    }
    let offset = (longitude / 15.0).round_ties_even().clamp(-12.0, 14.0);
    Ok(if offset == 0.0 {
        "Etc/GMT".into()
    } else {
        format!(
            "Etc/GMT{}{:.0}",
            if offset > 0.0 { "-" } else { "+" },
            offset.abs()
        )
    })
}
#[derive(serde::Deserialize)]
struct GeoResponse {
    status: String,
    timezone: Option<String>,
}
pub struct Internet {
    pub endpoint: String,
    pub timeout: Duration,
}
impl Default for Internet {
    fn default() -> Self {
        Self {
            endpoint: "http://ip-api.com/json/?fields=status,timezone".into(),
            timeout: Duration::from_secs(5),
        }
    }
}
impl Internet {
    pub fn lookup(&self, paths: &Paths) -> Option<String> {
        // The source intentionally suppresses every HTTP/JSON/zone validation failure.
        let config = ureq::Agent::config_builder()
            .timeout_connect(Some(self.timeout))
            .timeout_global(None)
            .max_idle_connections(0)
            .user_agent("openpilot-timed")
            .build();
        let agent = openpilot_http_transport::socket_timeout_agent(config, self.timeout);
        let mut response = agent
            .get(&self.endpoint)
            .header("accept-encoding", "identity")
            .call()
            .ok()?;
        let mut body = Vec::new();
        response
            .body_mut()
            .as_reader()
            .read_to_end(&mut body)
            .ok()?;
        let data: GeoResponse = serde_json::from_slice(&body).ok()?;
        let zone = data.timezone?;
        (data.status == "success" && !zone.is_empty() && paths.zoneinfo.join(&zone).is_file())
            .then_some(zone)
    }
}
