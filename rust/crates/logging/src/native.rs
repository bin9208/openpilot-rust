//! C++ cloudlog_e-compatible shared producer. This is separate from the Python producer.
//!
//! Callers matching C++ behavior must treat emission errors as best effort: the source ignores
//! printf/zmq_send failures. `for_runtime` holds process-wide state; call `close` after final
//! diagnostics on orderly shutdown to drain with the original 100 ms linger. Rust statics have
//! no C++ exit destructor. Arbitrary fork after initialization is unsupported (as in C++).
mod wire;
use crate::{Error, producer::Delivery, record::Level, site::Site};
use std::{
    env,
    io::{self, Write},
    sync::{Arc, Mutex, OnceLock},
};

#[derive(Clone)]
pub struct Logger(Arc<Shared>);
struct Shared {
    endpoint: String,
    version: String,
    device: String,
    state: Mutex<State>,
}
enum State {
    Pending,
    Ready {
        socket: zmq::Socket,
        context: wire::Context,
        console: Level,
    },
    Closed,
}
static RUNTIME: OnceLock<Mutex<Option<Logger>>> = OnceLock::new();

impl Logger {
    pub fn new(endpoint: String, version: &str, device: &str) -> Result<Self, Error> {
        Ok(Self(Arc::new(Shared {
            endpoint,
            version: version.into(),
            device: device.into(),
            state: Mutex::new(State::Pending),
        })))
    }
    /// The first call selects version, device and OPENPILOT_PREFIX for this process.
    pub fn for_runtime(version: &str, device: &str) -> Result<Self, Error> {
        let mut global = RUNTIME
            .get_or_init(|| Mutex::new(None))
            .lock()
            .map_err(|_| Error::Contract("native logger global lock poisoned"))?;
        if let Some(logger) = &*global {
            return Ok(logger.clone());
        }
        let prefix = match env::var("OPENPILOT_PREFIX") {
            Ok(value) => value,
            Err(env::VarError::NotPresent) => String::new(),
            Err(env::VarError::NotUnicode(_)) => {
                return Err(Error::Contract("logging IPC prefix is not UTF-8"));
            }
        };
        let logger = Self::new(format!("ipc:///tmp/logmessage{prefix}"), version, device)?;
        *global = Some(logger.clone());
        Ok(logger)
    }
    pub fn emit(&self, site: Site, level: Level, text: String) -> Result<Delivery, Error> {
        self.emit_with(site, site.file, level, text, None)
    }
    pub fn emit_named(
        &self,
        site: Site,
        source: &str,
        level: Level,
        text: String,
    ) -> Result<Delivery, Error> {
        self.emit_with(site, source, level, text, None)
    }
    pub fn emit_timestamp(
        &self,
        site: Site,
        level: Level,
        text: String,
        frame_id: Option<u32>,
    ) -> Result<Delivery, Error> {
        if text.is_empty() {
            return Ok(Delivery::Filtered);
        }
        let now = rustix::time::clock_gettime(rustix::time::ClockId::Boottime);
        let now_ns = now.tv_sec as u64 * 1_000_000_000 + now.tv_nsec as u64;
        self.emit_with(site, site.file, level, text, Some((now_ns, frame_id)))
    }
    fn emit_with(
        &self,
        site: Site,
        source: &str,
        level: Level,
        text: String,
        timestamp: Option<(u64, Option<u32>)>,
    ) -> Result<Delivery, Error> {
        if text.is_empty() {
            return Ok(Delivery::Filtered);
        }
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_| Error::Contract("native logger lock poisoned"))?;
        if let State::Pending = &*state {
            let context = zmq::Context::new();
            let socket = context.socket(zmq::PUSH)?;
            socket.set_linger(100)?;
            socket.connect(&self.0.endpoint)?;
            let context = wire::context(&self.0.version, &self.0.device)?;
            let console = match env::var("LOGPRINT").as_deref() {
                Ok("debug") => Level::Debug,
                Ok("info") => Level::Info,
                _ => Level::Warning,
            };
            *state = State::Ready {
                socket,
                context,
                console,
            };
        }
        match &*state {
            State::Ready {
                socket,
                context,
                console,
            } => {
                let created = rustix::time::clock_gettime(rustix::time::ClockId::Realtime);
                let created = created.tv_sec as f64 + created.tv_nsec as f64 * 1e-9;
                let text = wire::c_string(&text);
                let packet = wire::packet(site, source, level, text, created, context, timestamp)?;
                if level >= *console {
                    // Match printf failure suppression without recursively logging a console error.
                    let _ = writeln!(io::stdout().lock(), "{}: {}", wire::c_string(source), text);
                }
                match socket.send(packet, zmq::DONTWAIT) {
                    Ok(()) => Ok(Delivery::Sent),
                    Err(zmq::Error::EAGAIN) => Ok(Delivery::Dropped),
                    Err(error) => Err(error.into()),
                }
            }
            State::Closed => Err(zmq::Error::ENOTSOCK.into()),
            State::Pending => Err(Error::Contract("native logger initialization missing")),
        }
    }
    /// Close the connection shared by all clones. Before first emission this is a no-op.
    pub fn close(&self) -> Result<(), Error> {
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_| Error::Contract("native logger lock poisoned"))?;
        if matches!(&*state, State::Ready { .. }) {
            *state = State::Closed;
        }
        Ok(())
    }
}
