use crate::{pid, Error, ProcessLog};
use openpilot_logging::{
    log_site,
    record::{Level, Record},
};
use openpilot_params::{metadata, Params};
use std::{
    ffi::OsString,
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
        if crate::reconcile_native_service(std::path::Path::new(&self.command.identity))? {
            return Ok(());
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
    Ok(crate::native_identity::matches(
        pid,
        std::path::Path::new(identity),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::{os::unix::process::CommandExt, process::Command};

    fn wait_for_exec(child: &std::process::Child, executable: std::path::PathBuf) {
        use std::os::unix::fs::MetadataExt;
        let expected = fs::metadata(executable).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if fs::read(format!("/proc/{}/cmdline", child.id()))
                .is_ok_and(|bytes| !bytes.is_empty())
                && fs::metadata(format!("/proc/{}/exe", child.id())).is_ok_and(|actual| {
                    (actual.dev(), actual.ino()) == (expected.dev(), expected.ino())
                })
            {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "child did not exec");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn argument_containing_identity_is_not_an_executable_match() {
        let mut child = Command::new("/bin/sleep")
            .arg0("unrelated-openpilot-athena")
            .arg("30")
            .spawn()
            .unwrap();
        wait_for_exec(&child, PathBuf::from("/bin/sleep"));
        let result = matches_identity(child.id() as i32, "openpilot-athena");
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(
            !result.unwrap(),
            "argv text must not establish native ownership"
        );
    }

    #[test]
    fn obsolete_generation_does_not_match_current_executable() {
        let directory = tempfile::tempdir().unwrap();
        let old = directory.path().join("old");
        let new = directory.path().join("new");
        fs::create_dir(&old).unwrap();
        fs::create_dir(&new).unwrap();
        fs::copy("/bin/sleep", old.join("service")).unwrap();
        fs::copy("/bin/sleep", new.join("service")).unwrap();
        fs::File::open(old.join("service"))
            .unwrap()
            .sync_all()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut child = loop {
            match Command::new(old.join("service")).arg("30").spawn() {
                Ok(child) => break child,
                Err(error)
                    if error.raw_os_error() == Some(rustix::io::Errno::TXTBSY.raw_os_error())
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(5))
                }
                Err(error) => panic!("native fixture spawn failed: {error}"),
            }
        };
        wait_for_exec(&child, old.join("service"));
        let result = matches_identity(child.id() as i32, new.join("service").to_str().unwrap());
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(!result.unwrap());
    }
}
