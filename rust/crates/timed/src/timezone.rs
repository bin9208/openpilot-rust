use crate::{Commands, Error};
use openpilot_logging::{
    log_site,
    producer::Logger,
    record::{Level, Record},
};
use openpilot_params::Params;
use std::{
    collections::HashMap,
    ffi::OsString,
    io::Read,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

#[derive(Clone)]
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
#[derive(Clone)]
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
    pub fn lookup_until_stopped(&self, paths: &Paths, stop: &AtomicBool) -> Option<String> {
        let internet = self.clone();
        let paths = paths.clone();
        let worker = std::thread::Builder::new()
            .name("timed-geolocation".into())
            .spawn(move || internet.lookup(&paths))
            .ok()?;
        while !worker.is_finished() {
            if stop.load(Ordering::Relaxed) {
                // Only runtime shutdown sets this flag. The caller exits without
                // applying a result; process exit closes this worker's sockets.
                // Normal lookups always join, retaining per-I/O timeout semantics.
                return None;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        worker.join().ok().flatten()
    }
    pub fn lookup(&self, paths: &Paths) -> Option<String> {
        // The source intentionally suppresses every HTTP/JSON/zone validation failure.
        let config = ureq::Agent::config_builder()
            .timeout_connect(Some(self.timeout))
            .timeout_global(None)
            .max_idle_connections(0)
            .max_redirects(0)
            .user_agent("openpilot-timed")
            .build();
        let agent = openpilot_http_transport::socket_timeout_agent(config, self.timeout);
        let mut endpoint = url::Url::parse(&self.endpoint).ok()?;
        let mut redirects = HashMap::<String, usize>::new();
        let mut response = loop {
            let mut response = agent
                .get(endpoint.as_str())
                .header("accept-encoding", "identity")
                .call()
                .ok()?;
            if !matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
                if !response.status().is_success() {
                    return None;
                }
                break response;
            }
            let location = response
                .headers()
                .get("location")
                .or_else(|| response.headers().get("uri"))?
                .to_str()
                .ok()?;
            endpoint = endpoint.join(location).ok()?;
            let previous = redirects.get(endpoint.as_str()).copied().unwrap_or(0);
            // urllib limits repeated destinations independently of distinct URLs.
            if previous >= 4 || redirects.len() >= 10 {
                return None;
            }
            redirects.insert(endpoint.as_str().to_owned(), previous + 1);
            std::io::copy(&mut response.body_mut().as_reader(), &mut std::io::sink()).ok()?;
        };
        let mut body = Vec::new();
        response
            .body_mut()
            .as_reader()
            .read_to_end(&mut body)
            .ok()?;
        let [status, zone] = openpilot_logmessaged::string_fields(
            std::str::from_utf8(&body).ok()?,
            ["status", "timezone"],
        )
        .ok()?;
        let zone = zone?;
        (status.as_deref() == Some("success")
            && !zone.is_empty()
            && paths.zoneinfo.join(&zone).is_file())
        .then_some(zone)
    }
}
