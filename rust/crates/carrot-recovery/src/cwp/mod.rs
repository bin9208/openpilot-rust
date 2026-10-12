use crate::{config::Config, Error, Value};
use openpilot_cweb_push::helpers;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    net::{Ipv4Addr, UdpSocket},
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const ENABLE_PARAM: &str = "CwebPushRecoveryBoot";
const TIMEOUT: Duration = Duration::from_secs(4);

pub fn read_param(config: &Config, key: &str, default: &str) -> String {
    match fs::read(config.params.join("d").join(key)) {
        Ok(bytes) => {
            let value = String::from_utf8_lossy(&bytes)
                .replace("\r\n", "\n")
                .replace('\r', "\n");
            let value = helpers::strip(&value);
            if value.is_empty() {
                default.into()
            } else {
                value.into()
            }
        }
        Err(_) => default.into(),
    }
}

struct TemporaryParam(PathBuf);

impl Drop for TemporaryParam {
    fn drop(&mut self) {
        match fs::remove_file(&self.0) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => eprintln!("[recovery] temporary Params cleanup: {error}"),
        }
    }
}

pub(crate) fn write_param(config: &Config, key: &str, value: &str) -> Result<(), Error> {
    let directory = config.params.join("d");
    if !directory.is_dir() {
        return Err(Error::Source(format!(
            "params unavailable: {}",
            directory.display()
        )));
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o664)
        .open(config.params.join(".lock"))?;
    lock.lock()?;
    let mut random = [0_u8; 6];
    getrandom::fill(&mut random).map_err(|error| Error::Source(error.to_string()))?;
    let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let temporary = TemporaryParam(directory.join(format!(".tmp_{key}_{suffix}")));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o664)
        .open(&temporary.0)?;
    file.write_all(value.as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary.0, directory.join(key))?;
    File::open(directory)?.sync_all()?;
    Ok(())
}

pub fn device_id(config: &Config) -> String {
    for value in [
        read_param(config, "DongleId", ""),
        read_param(config, "HardwareSerial", ""),
    ] {
        let id = helpers::meaningful_id(&value);
        if !id.is_empty() {
            return id;
        }
    }
    let hostname = rustix::system::uname()
        .nodename()
        .to_string_lossy()
        .into_owned();
    let id = helpers::meaningful_id(&hostname);
    if id.is_empty() {
        "comma".into()
    } else {
        id
    }
}

fn report_url() -> String {
    let configured = std::env::var("CWEB_PUSH_REPORT_URL").unwrap_or_default();
    let configured = helpers::strip(&configured);
    if configured.is_empty() {
        helpers::default_url()
    } else {
        configured.into()
    }
}

fn endpoint(path: &str) -> String {
    let report = report_url();
    let base = report
        .strip_suffix("/report")
        .unwrap_or_else(|| report.trim_end_matches('/'));
    format!("{base}{path}")
}

fn request(path: &str, payload: &Value) -> Result<Value, Error> {
    let token = std::env::var("CWP_REPORT_TOKEN").unwrap_or_default();
    let token = helpers::strip(&token);
    let authorization = format!("Bearer {token}");
    let mut headers = vec![("Content-Type", "application/json")];
    if !token.is_empty() {
        headers.push(("Authorization", authorization.as_str()));
    }
    Ok(crate::upload::network::post(
        &endpoint(path),
        &headers,
        payload.encode()?.as_bytes(),
        TIMEOUT,
    ))
}

fn body(result: &Value) -> &Value {
    match result.get("body") {
        body @ Value::Object(_) => body,
        Value::Null
        | Value::Bool(_)
        | Value::Integer(_)
        | Value::Float(_)
        | Value::Text(_)
        | Value::Array(_) => &Value::Null,
    }
}

fn field(value: &mut Value, key: &str, replacement: Value) {
    if let Value::Object(fields) = value {
        match fields
            .iter_mut()
            .find(|(name, _)| name.iter().copied().eq(key.chars().map(u32::from)))
        {
            Some((_, value)) => *value = replacement,
            None => fields.push((key.chars().map(u32::from).collect(), replacement)),
        }
    }
}

pub fn status(config: &Config) -> Result<Value, Error> {
    let mut enabled = read_param(config, ENABLE_PARAM, "") == "1";
    let result = request(
        "/recovery/status",
        &Value::object([("deviceId", Value::text(&device_id(config)))]),
    )?;
    let body = body(&result);
    if !result.get("ok").truth() || !body.get("ok").truth() {
        let error = if result.get("error").truth() {
            result.get("error").clone()
        } else {
            Value::text("CWP unavailable")
        };
        return Ok(Value::object([
            ("ok", Value::Bool(false)),
            ("enabled", Value::Bool(enabled)),
            ("registered", Value::Null),
            ("state", Value::text("unavailable")),
            ("error", error),
        ]));
    }
    let registered = body.get("registered").truth();
    if enabled && !registered {
        write_param(config, ENABLE_PARAM, "0")?;
        enabled = false;
    }
    Ok(Value::object([
        ("ok", Value::Bool(true)),
        ("enabled", Value::Bool(enabled)),
        ("registered", Value::Bool(registered)),
        (
            "state",
            Value::text(if registered { "ready" } else { "unregistered" }),
        ),
    ]))
}

pub fn set_enabled(config: &Config, enabled: bool) -> Result<Value, Error> {
    let mut status = status(config)?;
    if enabled {
        if !status.get("ok").truth() {
            return Ok(status);
        }
        if !status.get("registered").truth() {
            field(&mut status, "ok", Value::Bool(false));
            field(&mut status, "error", Value::text("Not registered"));
            return Ok(status);
        }
    }
    write_param(config, ENABLE_PARAM, if enabled { "1" } else { "0" })?;
    field(&mut status, "ok", Value::Bool(true));
    field(&mut status, "enabled", Value::Bool(enabled));
    Ok(status)
}

pub fn local_ip() -> String {
    let address = (|| -> std::io::Result<Ipv4Addr> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.connect("8.8.8.8:80")?;
        match socket.local_addr()?.ip() {
            std::net::IpAddr::V4(value) => Ok(value),
            std::net::IpAddr::V6(_) => Err(std::io::Error::other("IPv4 route has IPv6 address")),
        }
    })();
    match address {
        Ok(address) if private_ip(address) => address.to_string(),
        Ok(_) | Err(_) => String::new(),
    }
}

fn private_ip(address: Ipv4Addr) -> bool {
    let number = u32::from(address);
    [
        (0x0000_0000, 8),
        (0x0a00_0000, 8),
        (0x7f00_0000, 8),
        (0xa9fe_0000, 16),
        (0xac10_0000, 12),
        (0xc000_0000, 29),
        (0xc000_00aa, 31),
        (0xc000_0200, 24),
        (0xc0a8_0000, 16),
        (0xc612_0000, 15),
        (0xc633_6400, 24),
        (0xcb00_7100, 24),
        (0xf000_0000, 4),
    ]
    .into_iter()
    .any(|(base, prefix)| number >> (32 - prefix) == base >> (32 - prefix))
}

enum IpSource {
    Route,
    #[cfg(test)]
    Fixture(PathBuf),
}

impl IpSource {
    fn read(&self) -> String {
        match self {
            Self::Route => local_ip(),
            #[cfg(test)]
            Self::Fixture(path) => match fs::read_to_string(path) {
                Ok(value) => helpers::strip(&value)
                    .parse::<Ipv4Addr>()
                    .map(|address| {
                        if private_ip(address) {
                            address.to_string()
                        } else {
                            String::new()
                        }
                    })
                    .unwrap_or_default(),
                Err(_) => String::new(),
            },
        }
    }
}

pub struct BootWorker {
    stop: Sender<()>,
    worker: Option<JoinHandle<Result<(), Error>>>,
}

impl BootWorker {
    pub fn start(config: &Config, port: u16) -> Result<Self, Error> {
        Self::spawn(config, port, IpSource::Route)
    }

    #[cfg(test)]
    fn start_with_ip_fixture(
        config: &Config,
        port: u16,
        path: &std::path::Path,
    ) -> Result<Self, Error> {
        let url =
            url::Url::parse(&report_url()).map_err(|error| Error::Source(error.to_string()))?;
        let loopback = url.host_str().is_some_and(|host| {
            host == "localhost"
                || host
                    .trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
        if !loopback || !matches!(url.scheme(), "http" | "https") {
            return Err(Error::Source(
                "CWP address fixture requires a loopback report URL".into(),
            ));
        }
        Self::spawn(config, port, IpSource::Fixture(path.into()))
    }

    fn spawn(config: &Config, port: u16, source: IpSource) -> Result<Self, Error> {
        let config = config.clone();
        let (stop, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("recovery-cwp-boot".into())
            .spawn(move || boot(&config, port, &source, &receiver))?;
        Ok(Self {
            stop,
            worker: Some(worker),
        })
    }

    pub fn stop(&mut self) -> Result<(), Error> {
        match self.stop.send(()) {
            Ok(()) | Err(_) => self.join(),
        }
    }

    pub fn join(&mut self) -> Result<(), Error> {
        match self.worker.take() {
            Some(worker) => worker
                .join()
                .map_err(|_| Error::Source("CWP boot worker panicked".into()))?,
            None => Ok(()),
        }
    }
}

impl Drop for BootWorker {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            eprintln!("[recovery] CWP boot cleanup: {error}");
        }
    }
}

fn stopped(receiver: &Receiver<()>, duration: Duration) -> bool {
    match receiver.recv_timeout(duration) {
        Ok(()) | Err(RecvTimeoutError::Disconnected) => true,
        Err(RecvTimeoutError::Timeout) => false,
    }
}

fn boot(config: &Config, port: u16, source: &IpSource, stop: &Receiver<()>) -> Result<(), Error> {
    if read_param(config, ENABLE_PARAM, "") != "1" {
        return Ok(());
    }
    let started = Instant::now();
    let mut candidate = String::new();
    while started.elapsed() < Duration::from_secs(120) {
        if stopped(stop, Duration::ZERO) {
            return Ok(());
        }
        let ip = source.read();
        if ip.is_empty() || ip != candidate {
            candidate = ip;
            if stopped(stop, Duration::from_secs(2)) {
                return Ok(());
            }
            continue;
        }
        let result = request(
            "/recovery/boot",
            &Value::object([
                ("deviceId", Value::text(&device_id(config))),
                ("ip", Value::text(&ip)),
                ("port", Value::integer(port)),
            ]),
        )?;
        let body = body(&result);
        if result.get("ok").truth() && body.get("ok").truth() {
            let pushed = if body.get("pushed").truth() {
                body.get("pushed").int()?
            } else {
                Value::integer(0).int()?
            };
            let registered = body.get("registered").truth();
            if !registered {
                write_param(config, ENABLE_PARAM, "0")?;
            }
            println!(
                "[recovery] CWP boot registered={} pushed={pushed}",
                if registered { "True" } else { "False" }
            );
            return Ok(());
        }
        if stopped(stop, Duration::from_secs(5)) {
            return Ok(());
        }
    }
    println!("[recovery] CWP boot unavailable");
    Ok(())
}

#[cfg(test)]
mod tests;
