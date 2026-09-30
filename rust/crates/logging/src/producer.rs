//! Per-thread source-compatible nonblocking PUSH transport and console routing.
use crate::{
    context::{ContextGuard, GlobalContext, LocalContext},
    record::{format_record, Level, Record},
    site::Site,
    Error, Fields,
};
use std::{
    env,
    io::{self, Write},
    sync::Arc,
};

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
        Self::new(format!("ipc:///tmp/logmessage{prefix}"))
    }
    pub fn bind_global(&self, fields: Fields) -> Result<(), Error> {
        self.global.bind(fields)
    }
    pub fn logger(&self) -> Logger {
        Logger {
            factory: self.clone(),
            local: self.global.local(),
            state: Connection::Disconnected,
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
}
impl Logger {
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
        if record.level < Level::Debug {
            return Ok(Delivery::Filtered);
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
        self.send(&packet)
    }
    fn send(&mut self, packet: &[u8]) -> Result<Delivery, Error> {
        let pid = std::process::id();
        let reconnect = match &self.state {
            Connection::Disconnected => true,
            Connection::Connected(transport) => transport.pid != pid,
            Connection::Closed(owner) if *owner == pid => return Err(zmq::Error::ENOTSOCK.into()),
            Connection::Closed(_) => true,
        };
        if reconnect {
            self.release();
            let context = zmq::Context::new();
            let socket = context.socket(zmq::PUSH)?;
            socket.set_linger(10)?;
            socket.connect(&self.factory.endpoint)?;
            // zmq::Socket retains a Context clone, so its last drop closes the socket before ctx_term.
            self.state = Connection::Connected(Transport { socket, pid });
        }
        let Connection::Connected(transport) = &self.state else {
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
        match std::mem::replace(&mut self.state, Connection::Disconnected) {
            Connection::Disconnected => None,
            Connection::Closed(pid) => Some(pid),
            Connection::Connected(transport) => {
                let pid = transport.pid;
                if pid != std::process::id() {
                    // After fork, the inherited libzmq I/O threads and locks are unusable. Like
                    // pyzmq _term/close's PID guard, skip both socket close and context termination.
                    // The child forgets only its copied handles; the parent's ownership is unchanged.
                    std::mem::forget(transport);
                }
                Some(pid)
            }
        }
    }
    pub fn close(&mut self) {
        if let Some(pid) = self.release() {
            self.state = Connection::Closed(pid);
        }
    }
}
impl Drop for Logger {
    fn drop(&mut self) {
        self.release();
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
