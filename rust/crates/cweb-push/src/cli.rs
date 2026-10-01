use crate::{helpers, Config, Error};
use std::path::PathBuf;

pub struct Options {
    pub config: Config,
    pub once: bool,
    pub interval: f64,
    pub fixture_ip: Option<PathBuf>,
}
fn variable(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.into())
}
fn number<T: std::str::FromStr>(name: &str, value: &str) -> Result<T, Error> {
    value
        .parse()
        .map_err(|_| Error::Contract(format!("invalid {name}: {value}")))
}
impl Options {
    pub fn parse() -> Result<Option<Self>, Error> {
        let mut config = Config {
            report_url: variable("CWEB_PUSH_REPORT_URL", &helpers::default_url()),
            iface: variable("CWEB_PUSH_IFACE", "wlan0"),
            port: number("port", &variable("CWEB_PUSH_PORT", "7000"))?,
            timeout_s: number("timeout", &variable("CWEB_PUSH_TIMEOUT_S", "4"))?,
            heartbeat_interval_s: number(
                "heartbeat interval",
                &variable("CWEB_PUSH_HEARTBEAT_INTERVAL_S", "10"),
            )?,
            debounce_s: number("debounce", &variable("CWEB_PUSH_DEBOUNCE_S", "1"))?,
            ..Config::default()
        };
        let mut heartbeat = std::env::var("CWEB_PUSH_HEARTBEAT_URL").ok();
        let mut interval = number("interval", &variable("CWEB_PUSH_INTERVAL_S", "5"))?;
        let mut once = false;
        let mut fixture_ip = None;
        let mut arguments = std::env::args().skip(1);
        while let Some(argument) = arguments.next() {
            let (name, attached) = argument
                .split_once('=')
                .map_or((argument.as_str(), None), |(name, value)| {
                    (name, Some(value))
                });
            if attached.is_some() && matches!(name, "--once" | "--dry-run" | "--help" | "-h") {
                return Err(Error::Contract(format!("unexpected value for {name}")));
            }
            match name {
                "--help" | "-h" => {
                    println!("openpilot-cweb-push [--once] [--dry-run] [--url URL] [--heartbeat-url URL] [--iface IFACE] [--port PORT] [--interval SECONDS] [--heartbeat-interval SECONDS] [--debounce SECONDS] [--timeout SECONDS]\n--fixture-ip FILE redirects address discovery to an owned file and requires loopback report URLs.");
                    return Ok(None);
                }
                "--once" => once = true,
                "--dry-run" => config.dry_run = true,
                name => {
                    let value = attached
                        .map(str::to_owned)
                        .or_else(|| arguments.next())
                        .ok_or_else(|| Error::Contract(format!("missing value for {name}")))?;
                    match name {
                        "--url" => config.report_url = value,
                        "--heartbeat-url" => heartbeat = Some(value),
                        "--iface" => config.iface = value,
                        "--port" => config.port = number(name, &value)?,
                        "--interval" => interval = number(name, &value)?,
                        "--heartbeat-interval" => {
                            config.heartbeat_interval_s = number(name, &value)?
                        }
                        "--debounce" => config.debounce_s = number(name, &value)?,
                        "--timeout" => config.timeout_s = number(name, &value)?,
                        "--fixture-ip" => fixture_ip = Some(PathBuf::from(value)),
                        _ => return Err(Error::Contract(format!("unknown option: {name}"))),
                    }
                }
            }
        }
        config.heartbeat_url = heartbeat
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| helpers::heartbeat_url(&config.report_url));
        if fixture_ip.is_some() {
            for value in [&config.report_url, &config.heartbeat_url] {
                let url =
                    url::Url::parse(value).map_err(|error| Error::Contract(error.to_string()))?;
                if url.scheme() != "http" || url.host_str() != Some("127.0.0.1") {
                    return Err(Error::Contract(
                        "fixture requires HTTP loopback endpoints".into(),
                    ));
                }
            }
        }
        Ok(Some(Self {
            config,
            once,
            interval,
            fixture_ip,
        }))
    }
}
