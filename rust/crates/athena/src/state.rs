use crate::{queue::Uploads, Error};
use openpilot_hardware_info::paths::Paths;
use openpilot_logging::producer::{Factory, Logger};
use openpilot_params::Params;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, MutexGuard,
    },
    thread::JoinHandle,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Clone)]
pub struct Config {
    pub host: String,
    pub handlers: usize,
    pub log_root: PathBuf,
    pub swaglog_root: PathBuf,
    pub stats_root: PathBuf,
    pub persist_root: PathBuf,
    pub basedir: PathBuf,
    pub process_launcher: PathBuf,
    pub pc: bool,
}
impl Config {
    pub fn for_runtime() -> Result<Self, Error> {
        let paths = Paths::default();
        Ok(Self {
            host: std::env::var("ATHENA_HOST").unwrap_or_else(|_| "wss://athena.comma.ai".into()),
            handlers: std::env::var("HANDLER_THREADS")
                .unwrap_or_else(|_| "4".into())
                .parse::<i64>()
                .map_err(|_| Error::Contract("HANDLER_THREADS must be an integer"))?
                .try_into()
                .unwrap_or(0),
            log_root: paths.log_root()?.into(),
            swaglog_root: paths.swaglog_root()?.into(),
            stats_root: paths.stats_root()?.into(),
            persist_root: paths.persist_root()?.into(),
            basedir: std::env::var_os("OPENPILOT_BASEDIR")
                .map_or_else(std::env::current_dir, |path| Ok(path.into()))?,
            pc: paths.pc,
            process_launcher: std::env::current_exe()?.with_file_name("openpilot-process-child"),
        })
    }
}

#[derive(Clone, Default)]
pub struct Stop(Arc<AtomicBool>);
impl Stop {
    pub fn requested(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
    pub fn request(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn signal_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.0)
    }
    pub fn wait(&self, duration: Duration) {
        let start = std::time::Instant::now();
        while !self.requested() && start.elapsed() < duration {
            std::thread::sleep(
                duration
                    .saturating_sub(start.elapsed())
                    .min(Duration::from_millis(100)),
            );
        }
    }
}

pub struct Shared {
    pub config: Config,
    pub params: Params,
    pub uploads: Mutex<Uploads>,
    pub available: Condvar,
    pub factory: Factory,
    pub proxies: Mutex<Vec<JoinHandle<()>>>,
    pub requests: crate::mailbox::Mailbox<(String, bool)>,
    pub replies: crate::mailbox::Mailbox<String>,
    pub low_priority: crate::mailbox::Mailbox<String>,
    pub log_responses: crate::mailbox::Mailbox<String>,
    pub upload_agent: ureq::Agent,
    pub attributes: Mutex<crate::forwarding::Attributes>,
}
impl Shared {
    pub fn new(config: Config) -> Result<Self, Error> {
        Ok(Self {
            config,
            params: Params::for_runtime()?,
            uploads: Mutex::new(Uploads::default()),
            available: Condvar::new(),
            factory: Factory::for_runtime()?,
            proxies: Mutex::new(Vec::new()),
            requests: Default::default(),
            replies: Default::default(),
            low_priority: Default::default(),
            log_responses: Default::default(),
            upload_agent: crate::http_socket::agent(),
            attributes: Default::default(),
        })
    }
    pub fn uploads(&self) -> Result<MutexGuard<'_, Uploads>, Error> {
        self.uploads
            .lock()
            .map_err(|_| Error::Contract("upload state lock poisoned"))
    }
    pub fn cache(&self) -> Result<(), Error> {
        self.params
            .put("AthenadUploadQueue", &self.uploads()?.cache()?)?;
        Ok(())
    }
    pub fn text(&self, key: &str, logger: &mut Logger) -> Result<Option<String>, Error> {
        Ok(openpilot_params_typed::get_string(
            &self.params,
            key,
            logger,
        )?)
    }
}

pub fn read(params: &Params, key: &str) -> Result<Option<Vec<u8>>, Error> {
    match params.get(key) {
        Ok(value) => Ok(value.filter(|value| !value.is_empty())),
        Err(openpilot_params::Error::Io(_)) => Ok(None),
        Err(error) => Err(error.into()),
    }
}
pub fn remove(params: &Params, key: &str) -> Result<(), Error> {
    match params.remove(key) {
        Ok(()) | Err(openpilot_params::Error::Io(_)) => Ok(()),
        Err(error) => Err(error.into()),
    }
}
pub fn boolean(params: &Params, key: &str) -> Result<bool, Error> {
    Ok(read(params, key)?.is_some_and(|bytes| bytes == b"1"))
}
pub fn now_ms() -> Result<i64, Error> {
    i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())
        .map_err(|_| Error::Contract("wall clock overflow"))
}
pub fn mono_ns() -> Result<i64, Error> {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    now.tv_sec
        .checked_mul(1_000_000_000)
        .and_then(|seconds| seconds.checked_add(now.tv_nsec))
        .ok_or(Error::Contract("monotonic clock overflow"))
}
