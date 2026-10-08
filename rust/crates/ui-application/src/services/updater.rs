//! UI update requests bind the manager-owned native updater, never a command-line substring.
use openpilot_cereal::log_capnp::manager_state;
use rustix::{
    fd::OwnedFd,
    process::{kill_process, pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
};
use std::{
    io,
    path::{Path, PathBuf},
};
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub enum Request {
    Check,
    Download,
}
#[derive(Clone, Copy, Debug, Default)]
pub enum Strategy {
    #[default]
    PidfdWhenAvailable,
    RecheckedPid,
}
#[derive(Clone, Copy, Debug, serde::Serialize, PartialEq, Eq)]
pub enum Method {
    Pidfd,
    RecheckedPid,
}
#[derive(Clone, Copy, Debug, serde::Serialize, PartialEq, Eq)]
pub enum Unavailable {
    NotRunning,
    Gone,
    IdentityChanged,
}
#[derive(Debug, serde::Serialize, PartialEq, Eq)]
pub enum Outcome {
    Sent { pid: i32, method: Method },
    Unavailable(Unavailable),
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Cereal(#[from] capnp::Error),
    #[error(transparent)]
    Text(#[from] std::str::Utf8Error),
    #[error("updater signal contract: {0}")]
    Contract(&'static str),
}
pub enum Binding {
    Ready(BoundUpdater),
    Unavailable(Unavailable),
}
pub struct BoundUpdater {
    pid: Pid,
    executable: PathBuf,
    start: u64,
    pidfd: Option<OwnedFd>,
}
fn disappeared(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::InvalidInput
    )
}
fn start_time(pid: Pid) -> Result<u64, Error> {
    let text = std::fs::read_to_string(format!("/proc/{}/stat", pid.as_raw_nonzero()))?;
    text.rsplit_once(") ")
        .and_then(|(_, fields)| fields.split_whitespace().nth(19))
        .and_then(|value| value.parse().ok())
        .ok_or(Error::Contract("process start time missing"))
}
fn executable(pid: Pid) -> io::Result<PathBuf> {
    std::fs::read_link(format!("/proc/{}/exe", pid.as_raw_nonzero()))
}
pub fn bind(
    manager: manager_state::Reader<'_>,
    expected: &Path,
    strategy: Strategy,
) -> Result<Binding, Error> {
    let mut selected = None;
    for process in manager.get_processes()? {
        if process.get_name()?.to_str()? == "updated"
            && process.get_running()
            && process.get_pid() > 0
        {
            selected = Pid::from_raw(process.get_pid());
            break;
        }
    }
    let Some(pid) = selected else {
        return Ok(Binding::Unavailable(Unavailable::NotRunning));
    };
    if expected.file_name() != Some(std::ffi::OsStr::new("openpilot-updated")) {
        return Err(Error::Contract("expected native openpilot-updated path"));
    }
    let expected = expected.canonicalize()?;
    let pidfd = match strategy {
        Strategy::PidfdWhenAvailable => match pidfd_open(pid, PidfdFlags::empty()) {
            Ok(fd) => Some(fd),
            Err(rustix::io::Errno::NOSYS) => None,
            Err(rustix::io::Errno::SRCH) => return Ok(Binding::Unavailable(Unavailable::Gone)),
            Err(error) => return Err(io::Error::from(error).into()),
        },
        Strategy::RecheckedPid => None,
    };
    let observed = match executable(pid) {
        Ok(value) => value,
        Err(error) if disappeared(&error) => return Ok(Binding::Unavailable(Unavailable::Gone)),
        Err(error) => return Err(error.into()),
    };
    if observed != expected {
        return Ok(Binding::Unavailable(Unavailable::IdentityChanged));
    }
    let start = match start_time(pid) {
        Ok(value) => value,
        Err(Error::Io(error)) if disappeared(&error) => {
            return Ok(Binding::Unavailable(Unavailable::Gone))
        }
        Err(error) => return Err(error),
    };
    Ok(Binding::Ready(BoundUpdater {
        pid,
        executable: expected,
        start,
        pidfd,
    }))
}
impl BoundUpdater {
    pub fn send(&self, request: Request) -> Result<Outcome, Error> {
        let signal = match request {
            Request::Check => Signal::USR1,
            Request::Download => Signal::HUP,
        };
        let observed = match executable(self.pid) {
            Ok(value) => value,
            Err(error) if disappeared(&error) => {
                return Ok(Outcome::Unavailable(Unavailable::Gone))
            }
            Err(error) => return Err(error.into()),
        };
        let start = match start_time(self.pid) {
            Ok(value) => value,
            Err(Error::Io(error)) if disappeared(&error) => {
                return Ok(Outcome::Unavailable(Unavailable::Gone))
            }
            Err(error) => return Err(error),
        };
        if observed != self.executable || start != self.start {
            return Ok(Outcome::Unavailable(Unavailable::IdentityChanged));
        }
        let (result, method) = if let Some(fd) = &self.pidfd {
            (pidfd_send_signal(fd, signal), Method::Pidfd)
        } else {
            (kill_process(self.pid, signal), Method::RecheckedPid)
        };
        match result {
            Ok(()) => Ok(Outcome::Sent {
                pid: self.pid.as_raw_nonzero().get(),
                method,
            }),
            Err(rustix::io::Errno::SRCH) => Ok(Outcome::Unavailable(Unavailable::Gone)),
            Err(error) => Err(io::Error::from(error).into()),
        }
    }
}
pub fn send(
    manager: manager_state::Reader<'_>,
    expected: &Path,
    request: Request,
) -> Result<Outcome, Error> {
    match bind(manager, expected, Strategy::PidfdWhenAvailable)? {
        Binding::Ready(target) => target.send(request),
        Binding::Unavailable(reason) => Ok(Outcome::Unavailable(reason)),
    }
}
