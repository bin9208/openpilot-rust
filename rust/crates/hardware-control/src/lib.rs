#![forbid(unsafe_code)]

mod controls;
mod initialization;
mod io_policy;
mod native;

pub use io_policy::{gpio_init, gpio_set, sudo_write};
pub use native::{Commands, LinuxPlatform, ProcessCommands};
use std::{collections::HashMap, io};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{message}")]
    Io { source: io::Error, message: String },
    #[error("{0}")]
    Other(String),
    #[error("{}", command_error(.argv, *.status))]
    Command { argv: Vec<String>, status: i32 },
}

impl Error {
    pub fn io(source: io::Error, path: &str) -> Self {
        let message = match source.raw_os_error() {
            Some(errno) => {
                let text = source.to_string();
                let description = text.split(" (os error ").next().unwrap_or(&text);
                format!("[Errno {errno}] {description}: '{path}'")
            }
            None => source.to_string(),
        };
        Self::Io { source, message }
    }
    fn is_permission(&self) -> bool {
        matches!(self, Self::Io { source, .. } if source.kind() == io::ErrorKind::PermissionDenied)
    }
    fn is_missing(&self) -> bool {
        matches!(self, Self::Io { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Shell(String),
    Call(Vec<String>),
    Output(Vec<String>),
}

#[derive(Debug)]
pub struct CommandOutput {
    pub status: i32,
    pub stdout: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
pub enum AmplifierAction<'a> {
    Shutdown(bool),
    Initialize(&'a str),
}

pub trait Platform {
    fn read(&mut self, path: &str) -> Result<String, Error>;
    fn write(&mut self, path: &str, value: &str) -> Result<(), Error>;
    fn command(&mut self, command: &Command) -> Result<CommandOutput, Error>;
    fn print(&mut self, value: &str) -> Result<(), Error>;
    fn sleep(&mut self, seconds: f64) -> Result<(), Error>;
    fn monotonic(&mut self) -> Result<f64, Error>;
    fn touch(&mut self, path: &str) -> Result<(), Error>;
    fn sync(&mut self) -> Result<(), Error>;
    fn c3x_lite(&mut self) -> Result<bool, Error>;
    fn amplifier(&mut self, action: AmplifierAction<'_>) -> Result<bool, Error>;
}

pub struct HardwareControl {
    model: Option<String>,
    amplifier_enabled: Option<bool>,
    irq_actions: HashMap<String, Vec<String>>,
}

impl HardwareControl {
    pub fn pc() -> Self {
        Self {
            model: None,
            amplifier_enabled: Some(false),
            irq_actions: HashMap::new(),
        }
    }
    pub fn board(model: &str) -> Self {
        Self {
            model: Some(model.into()),
            amplifier_enabled: None,
            irq_actions: HashMap::new(),
        }
    }
    pub fn has_internal_panda(&self) -> bool {
        self.model.is_some()
    }
    fn amplifier_enabled(&mut self, platform: &mut impl Platform) -> Result<bool, Error> {
        if let Some(enabled) = self.amplifier_enabled {
            return Ok(enabled);
        }
        let enabled = self.model.as_deref() != Some("mici") && !platform.c3x_lite()?;
        self.amplifier_enabled = Some(enabled);
        Ok(enabled)
    }
}

fn command_error(argv: &[String], status: i32) -> String {
    let command = format!(
        "[{}]",
        argv.iter()
            .map(|arg| format!("'{arg}'"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    if status < 0 {
        match nix::sys::signal::Signal::try_from(-status) {
            Ok(signal) => format!(
                "Command '{command}' died with <Signals.{signal:?}: {}>.",
                -status
            ),
            Err(_) => format!("Command '{command}' died with unknown signal {}.", -status),
        }
    } else {
        format!("Command '{command}' returned non-zero exit status {status}.")
    }
}
