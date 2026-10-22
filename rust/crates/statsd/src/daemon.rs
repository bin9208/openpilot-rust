use crate::{
    aggregation::{Flush, Metrics, Tags},
    Error, FILE_LIMIT, FLUSH_SECONDS, STATS_SOCKET,
};
use openpilot_logging::{
    log_site,
    producer::Logger,
    record::{Level, Record},
    Value,
};
use openpilot_messaging::{runtime::SubMaster, state::Options};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub use crate::clock::{Clock, SystemClock};
pub struct Configuration {
    pub endpoint: String,
    pub directory: PathBuf,
    pub source_root: PathBuf,
    pub device_type: Option<String>,
}
impl Configuration {
    pub fn for_runtime() -> Result<Self, Error> {
        let tici = Path::new("/TICI").is_file();
        let directory = if tici {
            PathBuf::from("/data/stats/")
        } else {
            let mut home = std::env::var_os("HOME")
                .or_else(|| home::home_dir().map(PathBuf::into_os_string))
                .ok_or(Error::Configuration("home directory unavailable"))?;
            home.push("/.comma");
            home.push(std::env::var_os("OPENPILOT_PREFIX").unwrap_or_default());
            home.push("/stats");
            PathBuf::from(home)
        };
        let endpoint = runtime_endpoint(std::env::var_os("ROUTE_UPLOAD_QA_IPC_ROOT"))?;
        Ok(Self {
            endpoint,
            directory,
            source_root: std::env::current_dir()?,
            device_type: None,
        })
    }
}

fn runtime_endpoint(qa_root: Option<std::ffi::OsString>) -> Result<String, Error> {
    Ok(match qa_root {
        None => STATS_SOCKET.into(),
        Some(path) => {
            let path = PathBuf::from(path);
            if path.parent() != Some(Path::new("/tmp"))
                || !path
                    .file_name()
                    .is_some_and(|name| name.as_encoded_bytes().starts_with(b"route-upload-ipc-"))
                || !fs::symlink_metadata(&path)?.is_dir()
            {
                return Err(Error::Configuration("invalid route-upload QA IPC root"));
            }
            format!(
                "ipc://{}/stats",
                path.to_str()
                    .ok_or(Error::Configuration("QA IPC root must be UTF-8"))?
            )
        }
    })
}
pub fn run(
    configuration: &Configuration,
    clock: &mut impl Clock,
    stop: &AtomicBool,
) -> Result<(), Error> {
    run_with_events(configuration, clock, stop, emit_event)
}
/// Run the same daemon with an injectable metric logging boundary. The fourth
/// input is independent of clocks/paths and preserves the source nested log-error handler.
pub fn run_with_events(
    configuration: &Configuration,
    clock: &mut impl Clock,
    stop: &AtomicBool,
    mut emit: impl FnMut(&mut Logger, &str, (&str, &str)) -> Result<(), Error>,
) -> Result<(), Error> {
    let factory = openpilot_logging::producer::Factory::for_runtime()?;
    let mut logger = factory.logger();
    let dongle_id = openpilot_params_typed::get_string(
        &openpilot_params::Params::for_runtime()?,
        "DongleId",
        &mut logger,
    )?;
    let context = zmq::Context::new();
    let socket = context.socket(zmq::PULL)?;
    socket.bind(&configuration.endpoint)?;
    fs::create_dir_all(&configuration.directory)?;
    let metadata = openpilot_runtime_version::get_build_metadata(&configuration.source_root)?;
    let tags = Tags::new(&metadata, || {
        Ok(match &configuration.device_type {
            Some(value) => value.clone(),
            None if Path::new("/TICI").is_file() => {
                fs::read_to_string("/sys/firmware/devicetree/base/model")?
                    .trim_matches('\0')
                    .rsplit("comma ")
                    .next()
                    .unwrap_or("")
                    .to_owned()
            }
            None => "pc".into(),
        })
    })?;
    let mut subscriber = SubMaster::for_runtime(&["deviceState"], Options::default())?;
    let mut storage = Storage::new(configuration.directory.clone());
    let mut last_flush = clock.monotonic()?;
    let mut metrics = Metrics::default();
    while !stop.load(Ordering::Relaxed) {
        let previous = started(&subscriber)?;
        subscriber.update(Duration::from_millis(100))?;
        loop {
            if stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            let packet = match socket.recv_bytes(zmq::DONTWAIT) {
                Ok(packet) => packet,
                Err(zmq::Error::EINTR) => continue,
                Err(zmq::Error::EAGAIN) => break,
                Err(error) => return Err(error.into()),
            };
            let metric = String::from_utf8(packet)?;
            crate::events::report(metrics.ingest(&metric), &metric, |name, field| {
                emit(&mut logger, name, field)
            })?;
        }
        let current = started(&subscriber)?;
        if clock.monotonic()? > last_flush + FLUSH_SECONDS || current != previous {
            let output = metrics.render(
                &tags,
                &Flush {
                    started: current,
                    timestamp_ns: clock.timestamp_ns()?,
                    dongle_id: dongle_id.clone(),
                },
            )?;
            last_flush = clock.monotonic()?;
            if storage.publish(&output)? == Publication::Full {
                logger.emit(
                    log_site!(),
                    Record::text(Level::Error, "stats dir full".into()),
                )?;
            }
        }
    }
    Ok(())
}
fn started(subscriber: &SubMaster) -> Result<bool, Error> {
    use openpilot_messaging::state::Error as StateError;
    match subscriber
        .state
        .topic("deviceState")?
        .event()?
        .which()
        .map_err(capnp::Error::from)?
    {
        openpilot_cereal::log_capnp::event::DeviceState(state) => Ok(state?.get_started()),
        _ => Err(StateError::Configuration("deviceState union expected").into()),
    }
}
pub fn emit_event(logger: &mut Logger, name: &str, field: (&str, &str)) -> Result<(), Error> {
    logger.emit(
        log_site!(),
        Record::event(
            name,
            vec![],
            [(field.0.into(), Value::Text(field.1.into()))]
                .into_iter()
                .collect(),
        )?,
    )?;
    Ok(())
}
#[derive(Debug, PartialEq, Eq)]
pub enum Publication {
    Empty,
    Full,
    Written(PathBuf),
}
pub struct Storage {
    directory: PathBuf,
    boot_uid: String,
    index: u64,
}
impl Storage {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            boot_uid: uuid::Uuid::new_v4().to_string()[..8].into(),
            index: 0,
        }
    }
    pub fn publish(&mut self, output: &[u32]) -> Result<Publication, Error> {
        let count = fs::read_dir(&self.directory)?
            .try_fold(0_usize, |count, entry| entry.map(|_| count + 1))?;
        if count >= FILE_LIMIT {
            return Ok(Publication::Full);
        }
        if output.is_empty() {
            return Ok(Publication::Empty);
        }
        let destination = self
            .directory
            .join(format!("{}_{}", self.boot_uid, self.index));
        if destination.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "stats file exists",
            )
            .into());
        }
        // NamedTemporaryFile(delete=False): even encoding/write/rename failures leave the
        // temporary file. No fsync is introduced into the source publication contract.
        let temporary = tempfile::Builder::new()
            .prefix("tmp")
            .tempfile_in(&self.directory)?;
        let (mut file, path) = temporary.keep().map_err(|error| error.error)?;
        let text: String = output
            .iter()
            .copied()
            .map(char::from_u32)
            .collect::<Option<_>>()
            .ok_or(Error::Unicode)?;
        file.write_all(text.as_bytes())?;
        drop(file);
        fs::rename(path, &destination)?;
        self.index += 1;
        Ok(Publication::Written(destination))
    }
}

#[cfg(test)]
mod qa_endpoint_tests {
    use super::*;
    #[test]
    fn default_and_owned_qa_endpoint_are_explicit_and_invalid_roots_fail() {
        assert_eq!(runtime_endpoint(None).unwrap(), STATS_SOCKET);
        let root = tempfile::Builder::new()
            .prefix("route-upload-ipc-")
            .tempdir_in("/tmp")
            .unwrap();
        assert_eq!(
            runtime_endpoint(Some(root.path().as_os_str().to_owned())).unwrap(),
            format!("ipc://{}/stats", root.path().display())
        );
        for path in [
            Path::new("/tmp"),
            Path::new("relative"),
            Path::new("/tmp/route-upload-ipc-missing"),
        ] {
            assert!(runtime_endpoint(Some(path.as_os_str().to_owned())).is_err());
        }
        let alias = root.path().with_extension("alias");
        std::os::unix::fs::symlink(root.path(), &alias).unwrap();
        assert!(runtime_endpoint(Some(alias.as_os_str().to_owned())).is_err());
        std::fs::remove_file(alias).unwrap();
    }
}
