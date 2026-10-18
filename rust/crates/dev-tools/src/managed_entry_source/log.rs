//! Independent SwagLogger context merge and original producer packet protocol.
use super::Error;
use serde_json::{json, Map, Value};
use std::{
    io::Write,
    time::{SystemTime, UNIX_EPOCH},
};
pub(super) struct Logger {
    // The handler owns its context independently of the body context reset.
    _context: zmq::Context,
    socket: Option<zmq::Socket>,
    local: Map<String, Value>,
    global: Map<String, Value>,
}
impl Logger {
    pub(super) fn new(endpoint: &str, global_daemon: Option<&Value>) -> Result<Self, Error> {
        let context = zmq::Context::new();
        let socket = context.socket(zmq::PUSH).map_err(Error::zmq)?;
        socket.set_linger(1000).map_err(Error::zmq)?;
        socket.connect(endpoint).map_err(Error::zmq)?;
        let mut global = Map::new();
        global.insert("inherited".into(), json!("global"));
        if let Some(daemon) = global_daemon.filter(|value| !value.is_null()) {
            global.insert("daemon".into(), daemon.clone());
        }
        let mut local = Map::new();
        local.insert("local".into(), json!("retained"));
        Ok(Self {
            _context: context,
            socket: Some(socket),
            local,
            global,
        })
    }
    pub(super) fn bind(&mut self, daemon: &str) {
        self.local.insert("daemon".into(), json!(daemon));
    }
    pub(super) fn close(&mut self) {
        self.socket = None;
    }
    pub(super) fn emit(
        &self,
        level: u8,
        message: &str,
        error: Option<&Error>,
    ) -> Result<(), Error> {
        // ConsoleHandler precedes the socket handler in the original logger.
        if level >= 30 {
            let _ = writeln!(std::io::stderr(), "{message}");
        }
        let socket = self
            .socket
            .as_ref()
            .ok_or_else(|| Error::new("ZMQError", "Socket operation on non-socket"))?;
        let mut context = self.local.clone();
        context.extend(self.global.clone());
        let level_name = match level {
            10 => "DEBUG",
            20 => "INFO",
            30 => "WARNING",
            _ => "ERROR",
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| Error::new("OSError", error.to_string()))?
            .as_secs_f64();
        let mut record = json!({"msg":message,"ctx":context,"level":level_name,"levelnum":level,"name":"swaglog","filename":"managed_entry_source.rs","lineno":0,"pathname":file!(),"module":"managed_entry_source","funcName":"run","host":rustix::system::uname().nodename().to_string_lossy(),"process":std::process::id(),"thread":rustix::thread::gettid().as_raw_pid(),"threadName":"MainThread","created":now});
        if let Some(error) = error {
            record["exc_info"] = json!(error.trace());
        }
        let mut packet = vec![level];
        packet.extend(
            serde_json::to_vec(&record)
                .map_err(|error| Error::new("ValueError", error.to_string()))?,
        );
        socket.send(packet, 0).map_err(Error::zmq)
    }
}
