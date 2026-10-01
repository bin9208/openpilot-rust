use crate::{signals::Wake, Error};
use openpilot_process_supervision::{CapturedChild, CapturedCommand};
use std::{
    ffi::OsString,
    io::Read,
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

pub trait Commands {
    fn run(&mut self, argv: &[String], cwd: Option<&Path>) -> Result<String, Error>;
}
pub fn command_environment(count: &str) -> Result<Vec<(OsString, OsString)>, Error> {
    let mut count = count
        .trim()
        .parse::<i64>()
        .map_err(|_| Error::Contract("invalid GIT_CONFIG_COUNT"))?;
    let mut env = Vec::new();
    for (key, value) in [
        ("gc.auto", "0"),
        ("gc.autoDetach", "false"),
        ("maintenance.auto", "false"),
    ] {
        env.push((format!("GIT_CONFIG_KEY_{count}").into(), key.into()));
        env.push((format!("GIT_CONFIG_VALUE_{count}").into(), value.into()));
        count = count
            .checked_add(1)
            .ok_or(Error::Contract("GIT_CONFIG_COUNT overflow"))?;
    }
    env.push(("GIT_CONFIG_COUNT".into(), count.to_string().into()));
    Ok(env)
}
pub fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).to_owned()).collect()
}

pub struct NativeCommands {
    pub launcher: PathBuf,
    pub wake: Arc<Wake>,
}
struct GroupChild {
    child: CapturedChild,
    group: rustix::process::Pid,
}
impl GroupChild {
    fn kill_group(&self) -> Result<(), Error> {
        match rustix::process::kill_process_group(self.group, rustix::process::Signal::KILL) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
            Err(error) => Err(std::io::Error::from(error).into()),
        }
    }
}
impl Drop for GroupChild {
    fn drop(&mut self) {
        if let Err(error) = self.kill_group() {
            eprintln!("updated command group cleanup: {error}");
        }
        if let Err(error) = self.child.process.wait() {
            eprintln!("updated command reap: {error}");
        }
    }
}
impl Commands for NativeCommands {
    fn run(&mut self, argv: &[String], cwd: Option<&Path>) -> Result<String, Error> {
        if self.wake.stopped() {
            return Err(Error::Interrupted);
        }
        let count = match std::env::var("GIT_CONFIG_COUNT") {
            Ok(value) => value,
            Err(std::env::VarError::NotPresent) => "0".into(),
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(Error::Contract("invalid GIT_CONFIG_COUNT"))
            }
        };
        let environment = command_environment(&count)?;
        let request = CapturedCommand {
            launcher: self.launcher.clone(),
            cwd: match cwd {
                Some(path) => path.into(),
                None => std::env::current_dir()?,
            },
            argv: argv.iter().map(OsString::from).collect(),
        };
        let (child, mut pipe) = request.spawn_session_merged_with_env(&environment)?;
        let pid = i32::try_from(child.process.id())
            .map_err(|_| Error::Contract("child PID out of range"))?;
        let group =
            rustix::process::Pid::from_raw(pid).ok_or(Error::Contract("invalid child PID"))?;
        let mut owned = GroupChild { child, group };
        let flags = rustix::fs::fcntl_getfl(&pipe).map_err(std::io::Error::from)?;
        rustix::fs::fcntl_setfl(&pipe, flags | rustix::fs::OFlags::NONBLOCK)
            .map_err(std::io::Error::from)?;
        let mut output = Vec::new();
        let mut buffer = [0; 16384];
        loop {
            if self.wake.stopped() {
                return Err(Error::Interrupted);
            }
            match pipe.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => output.extend_from_slice(&buffer[..count]),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error.into()),
            }
        }
        let status = loop {
            if self.wake.stopped() {
                return Err(Error::Interrupted);
            }
            if let Some(status) = owned.child.process.try_wait()? {
                break status;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        owned.kill_group()?;
        let output = String::from_utf8(output)?
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        if !status.success() {
            return Err(Error::Command {
                command: argv.into(),
                code: status
                    .code()
                    .unwrap_or_else(|| -status.signal().unwrap_or(1)),
                output,
            });
        }
        Ok(output)
    }
}
