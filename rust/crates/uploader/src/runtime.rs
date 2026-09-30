use crate::{
    clear_locks,
    http::{HttpTransfer, SigningKey},
    Backoff, Error, Event, EventSink, Uploader, XattrCache,
};
use openpilot_cereal::log_capnp::event;
use openpilot_logging::{
    log_site,
    producer::{Factory, Logger},
    record::Level,
    site::Site,
};
use openpilot_messaging::{runtime::SubMaster, state::Options};
use openpilot_params::Params;
use rand::Rng;
use std::{
    env,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

pub struct RuntimeEvents(Logger);
impl RuntimeEvents {
    pub fn new(logger: Logger) -> Self {
        Self(logger)
    }
    fn info(&mut self, site: Site, text: String) {
        self.emit(Event::text(site, Level::Info, text));
    }
}
impl EventSink for RuntimeEvents {
    fn emit(&mut self, event: Event) {
        match event.into_record() {
            Ok((site, record)) => {
                if let Err(error) = self.0.emit(site, record) {
                    eprintln!("uploader logging failed: {error}");
                }
            }
            Err(error) => eprintln!("uploader logging failed: {error}"),
        }
    }
}
pub fn persist_root() -> Result<PathBuf, Error> {
    if Path::new("/TICI").is_file() {
        return Ok("/persist".into());
    }
    let mut name = std::ffi::OsString::from(".comma");
    name.push(env::var_os("OPENPILOT_PREFIX").unwrap_or_default());
    Ok(home::home_dir()
        .ok_or(Error::Configuration("home directory unavailable"))?
        .join(name)
        .join("persist"))
}
fn string_param(params: &Params, key: &str) -> Result<Option<String>, Error> {
    Ok(params
        .get(key)?
        .filter(|bytes| !bytes.is_empty())
        .and_then(|bytes| String::from_utf8(bytes).ok()))
}
fn sleep(stop: &AtomicBool, seconds: f64) {
    let end = Instant::now() + Duration::from_secs_f64(seconds);
    while !stop.load(Ordering::Relaxed) {
        let remaining = end.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        thread::sleep(remaining.min(Duration::from_millis(20)));
    }
}
pub fn run(cycles: Option<u64>) -> Result<(), Error> {
    let allow_sleep = env::var("UPLOADER_SLEEP")
        .unwrap_or_else(|_| "1".into())
        .trim()
        .parse::<i128>()
        .map_err(|_| Error::Configuration("UPLOADER_SLEEP must be an integer"))?
        != 0;
    let force_wifi = env::var_os("FORCEWIFI").is_some();
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let mut events = RuntimeEvents(Factory::for_runtime()?.logger());
    let mut cpus = rustix::thread::CpuSet::new();
    for cpu in 0..4 {
        cpus.set(cpu);
    }
    if let Err(error) = rustix::thread::sched_setaffinity(None, &cpus) {
        events.emit(Event::exception(
            log_site!(),
            "failed to set core affinity",
            &error,
        ));
    }
    let root = openpilot_deleter::platform::log_root()
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    clear_locks(&root, &mut events)?;
    let params = Params::for_runtime()?;
    let Some(dongle_id) = string_param(&params, "DongleId")? else {
        events.info(log_site!(), "uploader missing dongle_id".into());
        return Err(Error::Configuration(
            "uploader can't start without dongle id",
        ));
    };
    let mut subscriptions = SubMaster::for_runtime(&["deviceState"], Options::default())?;
    let transfer = HttpTransfer {
        api_host: env::var("API_HOST").unwrap_or_else(|_| "https://api.commadotai.com".into()),
        dongle_id,
        key: SigningKey::load(&persist_root()?)?,
        version_header: env::current_dir()?.join("openpilot/common/version.h"),
        fake_upload: env::var_os("FAKEUPLOAD").is_some(),
        socket_timeout: Duration::from_secs(10),
    };
    let mut uploader = Uploader::new(root, transfer, XattrCache::default(), events);
    let mut backoff = Backoff::default();
    let mut remaining = cycles;
    while !stop.load(Ordering::Relaxed) {
        subscriptions.update(Duration::ZERO)?;
        let offroad = params.get_bool("IsOffroad")?;
        let event::Which::DeviceState(device) = subscriptions
            .state
            .topic("deviceState")?
            .event()?
            .which()
            .map_err(|_| Error::Configuration("invalid deviceState event"))?
        else {
            return Err(Error::Configuration("invalid deviceState event"));
        };
        let device = device?;
        let network_type = device
            .get_network_type()
            .map_or_else(|error| error.0, |value| value as u16);
        let delay = if network_type == 0 && !force_wifi {
            if offroad {
                60.0
            } else {
                5.0
            }
        } else {
            let requested = string_param(&params, "AthenadRecentlyViewedRoutes")?;
            let outcome = uploader.step(
                network_type,
                device.get_network_metered(),
                requested.as_deref(),
            )?;
            if outcome == crate::Outcome::Failure {
                uploader
                    .events
                    .info(log_site!(), format!("upload backoff {}", backoff.current()));
            }
            backoff.next(outcome, offroad, rand::rng().random())
        };
        if let Some(count) = &mut remaining {
            *count -= 1;
            if *count == 0 {
                break;
            }
        }
        if allow_sleep {
            sleep(&stop, delay);
        }
    }
    Ok(())
}
