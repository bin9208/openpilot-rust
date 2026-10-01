use crate::{
    pigeon::{self, Platform},
    serial::Serial,
    Error,
};
use chrono::{Datelike, Timelike};
use openpilot_hardware_control::{gpio_init, gpio_set, LinuxPlatform, ProcessCommands};
use openpilot_logging::{
    log_site,
    producer::{Factory, Logger},
    record::{Level, Record},
};
use std::{
    io::Read,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct Config {
    pub root: PathBuf,
    pub launcher: PathBuf,
    pub assist_url: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            root: "/".into(),
            launcher: "openpilot-native-launcher".into(),
            assist_url: "https://online-live2.services.u-blox.com/GetOnlineData.ashx".into(),
        }
    }
}
pub struct Native {
    pub config: Config,
    pub serial: Serial,
    pub stop: Arc<AtomicBool>,
    logger: Logger,
    hardware: LinuxPlatform,
}
pub fn monotonic() -> f64 {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    now.tv_sec as f64 + now.tv_nsec as f64 / 1e9
}
impl Native {
    pub fn open(config: Config, stop: Arc<AtomicBool>) -> Result<Self, Error> {
        let serial = Serial::open(&config.root.join("dev/ttyHS0"))?;
        let logger = Factory::for_runtime()?.logger();
        let hardware = LinuxPlatform::new(
            &config.root,
            ProcessCommands {
                launcher: config.launcher.clone(),
            },
        );
        Ok(Self {
            config,
            serial,
            stop,
            logger,
            hardware,
        })
    }
    fn interrupted(&self) -> Result<(), Error> {
        if self.stop.load(Ordering::Relaxed) {
            Err(Error::Interrupted)
        } else {
            Ok(())
        }
    }
}
impl Platform for Native {
    fn send(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.serial.send(bytes)
    }
    fn receive(&mut self) -> Result<Vec<u8>, Error> {
        self.interrupted()?;
        self.serial.receive()
    }
    fn baud(&mut self, value: u32) -> Result<(), Error> {
        self.serial.baud(value)
    }
    fn power(&mut self, enabled: bool) -> Result<(), Error> {
        for pin in [33, 34, 32] {
            gpio_init(&mut self.hardware, pin, true)?;
        }
        gpio_set(&mut self.hardware, 33, true)?;
        gpio_set(&mut self.hardware, 34, enabled)?;
        gpio_set(&mut self.hardware, 32, enabled)?;
        Ok(())
    }
    fn monotonic(&mut self) -> f64 {
        monotonic()
    }
    fn sleep(&mut self, seconds: f64) -> Result<(), Error> {
        let deadline = Instant::now() + Duration::from_secs_f64(seconds);
        loop {
            self.interrupted()?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(());
            }
            std::thread::sleep(remaining.min(Duration::from_millis(10)));
        }
    }
    fn log(&mut self, level: &str, text: &str) -> Result<(), Error> {
        let level = match level {
            "debug" => Level::Debug,
            "info" => Level::Info,
            "warning" => Level::Warning,
            "error" => Level::Error,
            _ => return Err(Error::Malformed("log level")),
        };
        self.logger
            .emit(log_site!(), Record::text(level, text.to_owned()))?;
        Ok(())
    }
    fn current_time(&mut self) -> Result<Option<Vec<u8>>, Error> {
        if !openpilot_timed::clock::valid(
            &openpilot_timed::clock::SystemClock,
            &self.config.root.join("lib/systemd/systemd"),
        )? {
            return Ok(None);
        }
        let now = chrono::Utc::now();
        let year = u16::try_from(now.year()).map_err(|_| Error::Malformed("UTC year"))?;
        let mut bytes = vec![0xb5, 0x62, 0x13, 0x40, 0x18, 0, 0x10, 0, 0, 0x80];
        bytes.extend(year.to_le_bytes());
        for field in [
            now.month(),
            now.day(),
            now.hour(),
            now.minute(),
            now.second(),
        ] {
            bytes.push(u8::try_from(field).map_err(|_| Error::Malformed("UTC field"))?);
        }
        bytes.extend([0, 0, 0, 0, 0, 30, 0, 0, 0, 0, 0, 0, 0]);
        Ok(Some(pigeon::add_checksum(bytes)))
    }
    fn token(&mut self) -> Result<Option<String>, Error> {
        let params = if self.config.root == std::path::Path::new("/") {
            openpilot_params::Params::for_runtime()?
        } else {
            openpilot_params::Params::open(
                &self.config.root.join("data/params"),
                &std::env::var("OPENPILOT_PREFIX")
                    .map_err(|_| Error::Malformed("fixture prefix"))?,
            )?
        };
        Ok(openpilot_params_typed::get_string(
            &params,
            "AssistNowToken",
            &mut self.logger,
        )?)
    }
    fn assist(&mut self, token: &str) -> Result<Vec<Vec<u8>>, Error> {
        let endpoint = self.config.assist_url.clone();
        let token = token.to_owned();
        let worker = std::thread::spawn(move || request_assist(&endpoint, &token));
        while !worker.is_finished() {
            self.sleep(0.01)?;
        }
        worker
            .join()
            .map_err(|_| Error::Boundary("AssistNow worker panicked".into()))?
    }
}
fn request_assist(endpoint: &str, token: &str) -> Result<Vec<Vec<u8>>, Error> {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("token", token)
        .append_pair("gnss", "gps,glo")
        .append_pair("datatype", "eph,alm,aux")
        .finish()
        .replace("%3A", ":")
        .replace("%2C", ",");
    let config = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(5)))
        .timeout_global(None)
        .max_redirects(30)
        .http_status_as_error(false)
        .build();
    let agent = openpilot_http_transport::socket_timeout_agent(config, Duration::from_secs(5));
    let mut response = agent.get(format!("{endpoint}?{query}")).call()?;
    if response.status().as_u16() != 200 {
        return Err(Error::Malformed("AssistNow HTTP status"));
    }
    let mut bytes = Vec::new();
    response.body_mut().as_reader().read_to_end(&mut bytes)?;
    pigeon::assist_messages(&bytes)
}
