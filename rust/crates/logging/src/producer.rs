//! Source-compatible nonblocking PUSH transport and console routing.
//! Ordinary loggers own per-thread connections; runtime emission is process-retained.
use crate::{
    context::{ContextGuard, GlobalContext, LocalContext},
    record::{format_record, Level, Record},
    site::Site,
    Error, Fields, Value,
};
use std::{
    env,
    io::{self, Write},
    sync::Arc,
};
mod runtime;
pub use runtime::{close_runtime, emit_runtime, RuntimeLogScope};

#[derive(Clone)]
pub struct Factory {
    endpoint: Arc<str>,
    host: Arc<str>,
    global: GlobalContext,
    console: Level,
}
impl Factory {
    pub fn new(endpoint: String) -> Result<Self, Error> {
        let host = rustix::system::uname()
            .nodename()
            .to_str()
            .map_err(|_| Error::Contract("hostname is not UTF-8"))?
            .to_owned();
        let console = match env::var_os("LOGPRINT") {
            None => Level::Warning,
            Some(value) => match value.to_str() {
                Some("debug") => Level::Debug,
                Some("info") => Level::Info,
                Some("warning") => Level::Warning,
                _ => Level::NotSet,
            },
        };
        Ok(Self {
            endpoint: endpoint.into(),
            host: host.into(),
            global: GlobalContext::default(),
            console,
        })
    }
    pub fn for_runtime() -> Result<Self, Error> {
        let prefix = match env::var("OPENPILOT_PREFIX") {
            Ok(value) => value,
            Err(env::VarError::NotPresent) => String::new(),
            Err(env::VarError::NotUnicode(_)) => {
                return Err(Error::Contract("logging IPC prefix is not UTF-8"))
            }
        };
        let factory = Self::new(format!("ipc:///tmp/logmessage{prefix}"))?;
        match env::var("OPENPILOT_LOG_CONTEXT") {
            Ok(context) => {
                let Value::Object(mut fields) = serde_json::from_str(&context)? else {
                    return Err(Error::Contract(
                        "inherited logging context must be an object",
                    ));
                };
                for key in [
                    "runtime_language",
                    "source_commit",
                    "source_tree",
                    "runtime_implementation",
                    "daemon",
                ] {
                    fields.shift_remove(key);
                }
                let executable = env::current_exe()?;
                let implementation = executable
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or(Error::Contract("logging implementation name is not UTF-8"))?;
                fields.insert(
                    "runtime_implementation".into(),
                    Value::Text(implementation.into()),
                );
                match env::var("MANAGER_DAEMON") {
                    Ok(name) => {
                        fields.insert("daemon".into(), Value::Text(name));
                    }
                    Err(env::VarError::NotPresent) => {}
                    Err(env::VarError::NotUnicode(_)) => {
                        return Err(Error::Contract("logging daemon name is not UTF-8"))
                    }
                }
                factory.bind_global(fields)?;
            }
            Err(env::VarError::NotPresent) => {}
            Err(env::VarError::NotUnicode(_)) => {
                return Err(Error::Contract("inherited logging context is not UTF-8"))
            }
        }
        Ok(factory)
    }
    pub fn bind_global(&self, fields: Fields) -> Result<(), Error> {
        self.global.bind(fields)
    }
    pub fn logger(&self) -> Logger {
        Logger {
            factory: self.clone(),
            local: self.global.local(),
            state: Connection::Disconnected,
            retained: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    Sent,
    Dropped,
    Filtered,
}
struct Transport {
    socket: zmq::Socket,
    pid: u32,
}
enum Connection {
    Disconnected,
    Connected(Transport),
    Closed(u32),
}

/// Rc-backed local context intentionally makes a logger neither Send nor Sync.
///
/// ```compile_fail
/// use openpilot_logging::producer::Factory;
/// let logger=Factory::new("ipc:///tmp/thread-contract".into()).unwrap().logger();
/// std::thread::spawn(move || drop(logger));
/// ```
/// ```compile_fail
/// fn needs_sync<T: Sync>() {}
/// needs_sync::<openpilot_logging::producer::Logger>();
/// ```
pub struct Logger {
    factory: Factory,
    local: LocalContext,
    state: Connection,
    retained: bool,
}
impl Logger {
    /// Create a caller-local context/formatter facade over the process-retained
    /// runtime connection. Dropping this facade never closes queued records.
    ///
    /// Factory initialization errors and local binding precedence match an
    /// ordinary runtime Logger. The first successful runtime emission selects
    /// the shared endpoint; retain RuntimeLogScope at the process root.
    /// Explicit close closes only this facade in the current PID.
    pub fn for_runtime_retained() -> Result<Self, Error> {
        let mut logger = Factory::for_runtime()?.logger();
        logger.retained = true;
        Ok(logger)
    }
    pub fn bind(&self, fields: Fields) {
        self.local.bind(fields)
    }
    pub fn context(&self, fields: Fields) -> ContextGuard {
        self.local.scoped(fields)
    }
    pub fn context_snapshot(&self) -> Result<Fields, Error> {
        self.local.snapshot()
    }
    pub fn emit(&mut self, site: Site, record: Record) -> Result<Delivery, Error> {
        let Some(packet) = self.prepare(site, record)? else {
            return Ok(Delivery::Filtered);
        };
        if self.retained {
            if let Connection::Closed(owner) = &self.state {
                if *owner == std::process::id() {
                    return Err(zmq::Error::ENOTSOCK.into());
                }
                // Like an ordinary Logger, a facade closed in its parent PID
                // can reconnect in the controlled fork child.
                self.state = Connection::Disconnected;
            }
            runtime::send_prepared(&self.factory, &packet)
        } else {
            self.state.send(&self.factory.endpoint, &packet)
        }
    }
    /// Format on the calling thread, before transport. Runtime emission uses a
    /// temporary logger only for its empty local/inherited global context; the
    /// Rc-backed local context never enters the shared process state.
    fn prepare(&self, site: Site, record: Record) -> Result<Option<Vec<u8>>, Error> {
        if record.level < Level::Debug {
            return Ok(None);
        }
        let metadata = site.metadata(&self.factory.host)?;
        if record.level >= self.factory.console {
            // StreamHandler suppresses console I/O errors before the IPC handler runs.
            let _ = write_console(&record);
        }
        let packet = format_record(
            record.level,
            record.message,
            self.local.snapshot()?,
            record.exception.as_deref(),
            &metadata,
        )?;
        Ok(Some(packet))
    }
    pub fn close(&mut self) {
        if self.retained {
            // No shared socket lives in the facade. Closing an unused or
            // inherited facade still closes this facade in the calling PID.
            self.state = Connection::Closed(std::process::id());
        } else {
            self.state.close();
        }
    }
}
impl Drop for Logger {
    fn drop(&mut self) {
        self.state.release();
    }
}

// Only the mechanical socket lifetime is shared. Both producer entrypoints use
// identical connect/send/release/close behavior, including the inherited-PID
// guard, EINTR retry, DONTWAIT, EAGAIN delivery result and ten-millisecond linger.
impl Connection {
    fn send(&mut self, endpoint: &str, packet: &[u8]) -> Result<Delivery, Error> {
        let pid = std::process::id();
        let reconnect = match self {
            Self::Disconnected => true,
            Self::Connected(transport) => transport.pid != pid,
            Self::Closed(owner) if *owner == pid => return Err(zmq::Error::ENOTSOCK.into()),
            Self::Closed(_) => true,
        };
        if reconnect {
            self.release();
            let context = zmq::Context::new();
            let socket = context.socket(zmq::PUSH)?;
            socket.set_linger(10)?;
            loop {
                match socket.connect(endpoint) {
                    Ok(()) => break,
                    Err(zmq::Error::EINTR) => continue,
                    Err(error) => return Err(error.into()),
                }
            }
            // Socket retains Context: socket close precedes context termination.
            *self = Self::Connected(Transport { socket, pid });
        }
        let Self::Connected(transport) = self else {
            return Err(Error::Contract("logger connection missing"));
        };
        loop {
            match transport.socket.send(packet, zmq::DONTWAIT) {
                Ok(()) => return Ok(Delivery::Sent),
                Err(zmq::Error::EINTR) => continue,
                Err(zmq::Error::EAGAIN) => return Ok(Delivery::Dropped),
                Err(error) => return Err(error.into()),
            }
        }
    }
    fn release(&mut self) -> Option<u32> {
        match std::mem::replace(self, Self::Disconnected) {
            Self::Disconnected => None,
            Self::Closed(pid) => Some(pid),
            Self::Connected(transport) => {
                let pid = transport.pid;
                if pid != std::process::id() {
                    // Inherited libzmq I/O threads/locks are unusable after fork.
                    // Like pyzmq's PID guard, skip close and context termination
                    // only for the child's copied handles, never the parent.
                    std::mem::forget(transport);
                }
                Some(pid)
            }
        }
    }
    fn close(&mut self) {
        if let Some(pid) = self.release() {
            *self = Self::Closed(pid);
        }
    }
}

fn write_console(record: &Record) -> Result<(), io::Error> {
    let mut stderr = io::stderr().lock();
    stderr.write_all(record.console.as_bytes())?;
    if let Some(details) = record.exception.as_deref().filter(|text| !text.is_empty()) {
        if !record.console.ends_with('\n') {
            stderr.write_all(b"\n")?;
        }
        stderr.write_all(details.as_bytes())?;
    }
    stderr.write_all(b"\n")?;
    stderr.flush()
}
