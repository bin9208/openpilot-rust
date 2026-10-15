//! Production adapter for the separately verified automatic process owner.
use crate::{
    auto::{self, Runtime},
    auto_process::ProcessOwner,
    backend::{Backend, Request},
    runtime::{self, Config, DIRECTORY, Error},
};
use openpilot_plannerd::signal::transport::Mode;
use serde_json::{Value, json};
use std::{
    fs, io,
    path::Path,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};
pub fn flag(config: &Config, name: &str) -> Result<bool, Error> {
    // Python catches OSError here, but a Unicode decoding failure is a worker
    // error. Do not turn malformed configuration bytes into a silent selection.
    let bytes = match fs::read(config.directory.join(name)) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(false),
    };
    let text = String::from_utf8(bytes)?;
    Ok(text.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)) == "1")
}
pub fn selection(config: &Config) -> Result<Request, Error> {
    let text = match fs::read_to_string(config.directory.join("neural_backend")) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    Ok(Request::parse(text.as_deref())?)
}
struct Adapter<'a, M, A, S, E> {
    config: &'a Config,
    stop: &'a AtomicBool,
    mode: &'a mut M,
    automatic: &'a mut A,
    spawn: &'a mut S,
    event: &'a mut E,
}
impl<M, A, S, E> Runtime for Adapter<'_, M, A, S, E>
where
    M: FnMut() -> Mode,
    A: FnMut() -> Result<Backend, Error>,
    S: FnMut(Backend) -> Result<Command, Error>,
    E: FnMut(&str, Value) -> Result<(), Error>,
{
    type Child = ProcessOwner;
    fn selection(&mut self) -> Result<Request, Error> {
        selection(self.config)
    }
    fn requested(&mut self) -> Result<bool, Error> {
        if self.config.directory == Path::new(DIRECTORY) {
            Ok((self.mode)().enabled())
        } else {
            flag(self.config, "enabled")
        }
    }
    fn neural_requested(&mut self) -> Result<bool, Error> {
        flag(self.config, "neural_enabled")
    }
    fn automatic_backend(&mut self) -> Result<Backend, Error> {
        (self.automatic)()
    }
    fn start(&mut self, backend: Backend) -> Result<ProcessOwner, Error> {
        ProcessOwner::spawn(&mut (self.spawn)(backend)?)
    }
    fn event(&mut self, backend: Backend, pid: u32) -> Result<(), Error> {
        (self.event)(
            "signalAutoBackend",
            json!({"backend":backend,"pid":pid,"control_permission":false}),
        )
    }
    fn now(&mut self) -> f64 {
        runtime::monotonic_ns() as f64 / 1e9
    }
    fn wait(&mut self, duration: Duration) {
        thread::sleep(duration);
    }
    fn interrupted(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }
}
pub fn run(
    config: &Config,
    stop: &AtomicBool,
    mode: &mut impl FnMut() -> Mode,
    automatic: &mut impl FnMut() -> Result<Backend, Error>,
    spawn: &mut impl FnMut(Backend) -> Result<Command, Error>,
    event: &mut impl FnMut(&str, Value) -> Result<(), Error>,
) -> Result<(), Error> {
    auto::run(
        &mut Adapter {
            config,
            stop,
            mode,
            automatic,
            spawn,
            event,
        },
        config.duration,
    )
}
