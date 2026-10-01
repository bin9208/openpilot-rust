use crate::{address, helpers, http::Http, Error, Payload, Platform, PostResult, Status};
use openpilot_logging::producer::{Factory, Logger};
use openpilot_params::Params;
use std::{
    io::Write,
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

pub struct Native {
    params: Params,
    logger: Logger,
    http: Http,
    stop: Arc<AtomicBool>,
    pub fixture_ip: Option<PathBuf>,
}
impl Native {
    pub fn new(fixture_ip: Option<PathBuf>, stop: Arc<AtomicBool>) -> Result<Self, Error> {
        Ok(Self {
            params: Params::for_runtime()?,
            logger: Factory::for_runtime()?.logger(),
            http: Http::new()?,
            fixture_ip,
            stop,
        })
    }
    fn param(&mut self, key: &str) -> String {
        openpilot_params_typed::get_string(&self.params, key, &mut self.logger)
            .ok()
            .flatten()
            .map(|value| helpers::strip(&value).to_owned())
            .unwrap_or_default()
    }
}
fn clock(id: rustix::time::ClockId) -> f64 {
    let value = rustix::time::clock_gettime(id);
    value.tv_sec as f64 + value.tv_nsec as f64 * 1e-9
}
impl Platform for Native {
    fn monotonic(&self) -> f64 {
        clock(rustix::time::ClockId::Monotonic)
    }
    fn wall_seconds(&self) -> f64 {
        clock(rustix::time::ClockId::Realtime)
    }
    fn uniform(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * rand::random::<f64>()
    }
    fn local_ip(&mut self, iface: &str) -> String {
        match &self.fixture_ip {
            Some(path) => std::fs::read_to_string(path)
                .map(|value| helpers::usable_ip(&value))
                .unwrap_or_default(),
            None => address::local_ip(iface),
        }
    }
    fn device_id(&mut self) -> String {
        let values = [self.param("DongleId"), self.param("HardwareSerial")];
        for value in values {
            let id = helpers::meaningful_id(&value);
            if !id.is_empty() {
                return id;
            }
        }
        let host = rustix::system::uname()
            .nodename()
            .to_string_lossy()
            .into_owned();
        if host.is_empty() {
            "comma".into()
        } else {
            host
        }
    }
    fn post(&mut self, url: &str, payload: &Payload, timeout_s: f64) -> Result<PostResult, Error> {
        self.http
            .post_stoppable(url, payload, timeout_s, &self.stop)
    }
    fn emit(&mut self, status: Status) -> Result<(), Error> {
        let mut output = std::io::stdout().lock();
        writeln!(output, "[cweb_push] {}", serde_json::to_string(&status)?)?;
        output.flush()?;
        Ok(())
    }
}
