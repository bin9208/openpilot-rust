use crate::{pid, Error, ProcessLog};
use openpilot_logging::{
    log_site,
    record::{Level, Record},
};
use openpilot_params::{metadata, Params};
use rustix::process::{test_kill_process, Pid};
use std::{
    ffi::OsString,
    fs,
    path::PathBuf,
    process::Child,
    sync::{Mutex, OnceLock},
};

pub enum ParamsSource {
    Runtime,
    Directory { root: PathBuf, prefix: String },
}

impl ParamsSource {
    fn open(&self) -> Result<Params, Error> {
        Ok(match self {
            Self::Runtime => Params::for_runtime()?,
            Self::Directory { root, prefix } => Params::open(root, prefix)?,
        })
    }
}

pub struct PersistentCommand {
    pub launcher: PathBuf,
    pub argv: Vec<OsString>,
    pub identity: String,
    pub pid_param: String,
}

pub struct PersistentDaemonProcess {
    command: PersistentCommand,
    source: ParamsSource,
    params: Option<Params>,
}

static DETACHED: OnceLock<Mutex<Vec<Child>>> = OnceLock::new();

impl PersistentDaemonProcess {
    pub fn new(command: PersistentCommand, source: ParamsSource) -> Self {
        Self {
            command,
            source,
            params: None,
        }
    }

    pub(crate) fn start(&mut self, name: &str, logger: &ProcessLog) -> Result<(), Error> {
        if self.params.is_none() {
            self.params = Some(self.source.open()?);
        }
        let params = self.params.as_ref().ok_or(Error::MissingParams)?;
        let info = metadata(&self.command.pid_param)
            .ok_or_else(|| openpilot_params::Error::UnknownKey(self.command.pid_param.clone()))?;
        if info.kind != 2 {
            return Err(Error::PidKeyType(self.command.pid_param.clone()));
        }
        let bytes = match params.get(&self.command.pid_param) {
            Ok(Some(bytes)) if !bytes.is_empty() => Some(bytes),
            Ok(_) | Err(openpilot_params::Error::Io(_)) => None,
            Err(error) => return Err(error.into()),
        };
        if let Some(bytes) = bytes {
            match pid::parse(&bytes)? {
                Some(pid) if matches_identity(pid, &self.command.identity)? => return Ok(()),
                Some(_) => {}
                None => {
                    logger.emit(log_site!(), Record::text(Level::Warning, format!(
                        "Failed to cast param {} with value={} from type t=<ParamKeyType.INT: 2>", self.command.pid_param, pid::bytes_repr(&bytes),
                    )))?;
                }
            }
        }
        logger.emit(
            log_site!(),
            Record::text(Level::Info, format!("starting daemon {name}")),
        )?;
        let mut detached = DETACHED
            .get_or_init(|| Mutex::new(Vec::new()))
            .lock()
            .map_err(|_| Error::ReaperPoisoned)?;
        let mut index = 0;
        while index < detached.len() {
            if detached[index].try_wait()?.is_some() {
                detached.swap_remove(index).wait()?;
            } else {
                index += 1;
            }
        }
        let child = crate::launch::spawn_persistent(&self.command.launcher, &self.command.argv)?;
        let pid = child.id();
        detached.push(child);
        match params.put(&self.command.pid_param, pid.to_string().as_bytes()) {
            Ok(()) | Err(openpilot_params::Error::Io(_)) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

fn matches_identity(pid: i32, identity: &str) -> Result<bool, Error> {
    if pid <= 0 {
        return Ok(false);
    }
    let probe = test_kill_process(Pid::from_raw(pid).ok_or(Error::PidRange)?);
    if probe.is_err() {
        return Ok(false);
    }
    let bytes = match fs::read(format!("/proc/{pid}/cmdline")) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(false),
    };
    let command = String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    Ok(command.contains(identity))
}
