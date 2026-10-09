use crate::Error;
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fs::Permissions,
    io::{Read, Write},
    os::{
        fd::{AsFd, AsRawFd, BorrowedFd},
        unix::{
            ffi::OsStringExt,
            fs::PermissionsExt,
            net::{UnixListener, UnixStream},
            process::CommandExt,
        },
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};
use tempfile::Builder;

#[derive(Clone, Debug)]
pub struct NativeCommand {
    pub launcher: PathBuf,
    pub basedir: PathBuf,
    pub cwd: PathBuf,
    pub argv: Vec<OsString>,
}

pub struct CapturedCommand {
    pub launcher: PathBuf,
    pub cwd: PathBuf,
    pub argv: Vec<OsString>,
}

pub struct CapturedChild {
    pub process: Child,
    _descriptor: tempfile::TempDir,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum LaunchRequest {
    Native {
        name: String,
        cwd: Vec<u8>,
        argv: Vec<Vec<u8>>,
    },
    Persistent {
        argv: Vec<Vec<u8>>,
    },
    Exec {
        cwd: Vec<u8>,
        argv: Vec<Vec<u8>>,
        handshake: Vec<u8>,
        #[serde(default)]
        new_session: bool,
        #[serde(default)]
        inherit_lock: bool,
    },
}

impl NativeCommand {
    pub(crate) fn spawn(&self, name: &str) -> Result<ChildHandle, Error> {
        let request = LaunchRequest::Native {
            name: name.into(),
            cwd: self.basedir.join(&self.cwd).into_os_string().into_vec(),
            argv: self
                .argv
                .iter()
                .map(|arg| arg.as_encoded_bytes().into())
                .collect(),
        };
        let mut descriptor = tempfile::NamedTempFile::new()?;
        serde_json::to_writer(descriptor.as_file_mut(), &request)?;
        let process = Command::new(&self.launcher)
            .arg(descriptor.path())
            .spawn()?;
        Ok(ChildHandle {
            process,
            _descriptor: descriptor,
        })
    }
}

pub(crate) struct ChildHandle {
    pub process: Child,
    _descriptor: tempfile::NamedTempFile,
}

enum StreamMode {
    PipedStdin,
    Captured,
    Stdout,
    Inherited,
    Redirected(Stdio, Stdio),
    SessionFiles(Stdio, Stdio),
}

impl CapturedCommand {
    pub fn spawn(&self) -> Result<CapturedChild, Error> {
        self.spawn_with_stdio(StreamMode::Captured, &[], false)
    }

    /// Capture separate byte streams under one post-spawn child/EOF deadline.
    pub fn capture(
        &self,
        timeout: Duration,
        lock: Option<BorrowedFd<'_>>,
    ) -> Result<std::process::Output, crate::CaptureError> {
        let mut child = match lock {
            Some(lock) => self.spawn_captured_with_lock(&[], lock)?,
            None => self.spawn()?,
        };
        crate::capture_output(&mut child.process, timeout)
    }

    /// Pass one borrowed lock into a child with separate pipes and no new session.
    pub fn spawn_captured_with_lock(
        &self,
        environment: &[(OsString, OsString)],
        lock: BorrowedFd<'_>,
    ) -> Result<CapturedChild, Error> {
        self.spawn_controlled(StreamMode::Captured, environment, false, Some(lock))
    }

    pub fn spawn_stdout(&self) -> Result<CapturedChild, Error> {
        self.spawn_with_stdio(StreamMode::Stdout, &[], false)
    }

    pub fn spawn_discarded(&self) -> Result<CapturedChild, Error> {
        self.spawn_with_stdio(
            StreamMode::Redirected(Stdio::null(), Stdio::null()),
            &[],
            false,
        )
    }

    pub fn spawn_redirected(&self, stdout: Stdio, stderr: Stdio) -> Result<CapturedChild, Error> {
        self.spawn_with_stdio(StreamMode::Redirected(stdout, stderr), &[], false)
    }

    /// Detach a new session with file outputs and EOF on stdin, matching Popen.
    pub fn spawn_session_redirected(
        &self,
        stdout: Stdio,
        stderr: Stdio,
    ) -> Result<CapturedChild, Error> {
        self.spawn_with_stdio(StreamMode::SessionFiles(stdout, stderr), &[], true)
    }

    pub fn spawn_piped_stdin(&self) -> Result<CapturedChild, Error> {
        self.spawn_with_stdio(StreamMode::PipedStdin, &[], false)
    }

    pub fn spawn_inherited(&self) -> Result<CapturedChild, Error> {
        self.spawn_with_stdio(StreamMode::Inherited, &[], false)
    }

    pub fn spawn_inherited_with_env(
        &self,
        environment: &[(OsString, OsString)],
    ) -> Result<CapturedChild, Error> {
        self.spawn_with_stdio(StreamMode::Inherited, environment, false)
    }

    /// Start an owned session with one pipe preserving stdout/stderr ordering.
    /// The caller owns group termination, including descendants after leader exit.
    pub fn spawn_session_merged_with_env(
        &self,
        environment: &[(OsString, OsString)],
    ) -> Result<(CapturedChild, std::io::PipeReader), Error> {
        let (reader, writer) = std::io::pipe()?;
        let stderr = writer.try_clone()?;
        let child = self.spawn_with_stdio(
            StreamMode::Redirected(writer.into(), stderr.into()),
            environment,
            true,
        )?;
        Ok((child, reader))
    }

    /// Pass exactly one repository lock's open-file description into an owned session.
    pub fn spawn_session_merged_with_lock(
        &self,
        environment: &[(OsString, OsString)],
        lock: BorrowedFd<'_>,
    ) -> Result<(CapturedChild, std::io::PipeReader), Error> {
        let (reader, writer) = std::io::pipe()?;
        let stderr = writer.try_clone()?;
        let child = self.spawn_controlled(
            StreamMode::Redirected(writer.into(), stderr.into()),
            environment,
            true,
            Some(lock),
        )?;
        Ok((child, reader))
    }

    fn spawn_with_stdio(
        &self,
        mode: StreamMode,
        environment: &[(OsString, OsString)],
        new_session: bool,
    ) -> Result<CapturedChild, Error> {
        self.spawn_controlled(mode, environment, new_session, None)
    }

    fn spawn_controlled(
        &self,
        mode: StreamMode,
        environment: &[(OsString, OsString)],
        new_session: bool,
        lock: Option<BorrowedFd<'_>>,
    ) -> Result<CapturedChild, Error> {
        crate::exec::validate_arguments(&self.argv)?;
        let directory = Builder::new()
            .prefix("op-exec-")
            .permissions(Permissions::from_mode(0o700))
            .tempdir_in("/tmp")?;
        let socket_path = directory.path().join("exec.sock");
        let listener = UnixListener::bind(&socket_path)?;
        listener.set_nonblocking(true)?;
        let request = LaunchRequest::Exec {
            cwd: self.cwd.as_os_str().as_encoded_bytes().into(),
            argv: self
                .argv
                .iter()
                .map(|arg| arg.as_encoded_bytes().into())
                .collect(),
            handshake: socket_path.into_os_string().into_vec(),
            new_session,
            inherit_lock: lock.is_some(),
        };
        let mut descriptor = tempfile::NamedTempFile::new_in(directory.path())?;
        serde_json::to_writer(descriptor.as_file_mut(), &request)?;
        let mut command = Command::new(&self.launcher);
        command.arg(descriptor.path());
        command.envs(environment.iter().map(|(key, value)| (key, value)));
        let command = match mode {
            StreamMode::PipedStdin => command.stdin(Stdio::piped()),
            StreamMode::Captured => command.stdout(Stdio::piped()).stderr(Stdio::piped()),
            StreamMode::Stdout => command.stdout(Stdio::piped()),
            StreamMode::Inherited => &mut command,
            StreamMode::Redirected(stdout, stderr) => command.stdout(stdout).stderr(stderr),
            StreamMode::SessionFiles(stdout, stderr) => {
                command.stdin(Stdio::null()).stdout(stdout).stderr(stderr)
            }
        };
        let mut child = command.spawn()?;
        if let Err(error) = wait_for_exec(&listener, &mut child, lock) {
            if lock.is_some() && new_session {
                let group = i32::try_from(child.id())
                    .ok()
                    .and_then(rustix::process::Pid::from_raw)
                    .ok_or(Error::PidRange)?;
                match rustix::process::kill_process_group(group, rustix::process::Signal::KILL) {
                    Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                    Err(error) => {
                        child.kill()?;
                        child.wait()?;
                        return Err(std::io::Error::from(error).into());
                    }
                }
            }
            let termination = child.kill();
            child.wait()?;
            match termination {
                Ok(()) => {}
                Err(kill_error) if kill_error.kind() == std::io::ErrorKind::InvalidInput => {}
                Err(kill_error) => return Err(kill_error.into()),
            }
            return Err(error);
        }
        Ok(CapturedChild {
            process: child,
            _descriptor: directory,
        })
    }
}

fn wait_for_exec(
    listener: &UnixListener,
    child: &mut Child,
    lock: Option<BorrowedFd<'_>>,
) -> Result<(), Error> {
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                if let Some(lock) = lock {
                    crate::lock_fd::send(&mut stream, lock)?;
                }
                let mut report = Vec::new();
                stream.take(5).read_to_end(&mut report)?;
                return match report.as_slice() {
                    [] => Ok(()),
                    [a, b, c, d] => {
                        Err(
                            std::io::Error::from_raw_os_error(i32::from_ne_bytes([*a, *b, *c, *d]))
                                .into(),
                        )
                    }
                    _ => Err(Error::LaunchProtocol("invalid exec handshake")),
                };
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if child.try_wait()?.is_some() {
                    return Err(Error::LaunchProtocol("helper exited before exec handshake"));
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error.into()),
        }
    }
}

pub fn run_child(input: impl Read) -> Result<(), Error> {
    let request: LaunchRequest = serde_json::from_reader(input)?;
    match request {
        LaunchRequest::Native { name, cwd, argv } => {
            let environment = crate::exec::environment(Some(&name))?;
            std::env::set_current_dir(PathBuf::from(OsString::from_vec(cwd)))?;
            let arguments: Vec<_> = argv.into_iter().map(OsString::from_vec).collect();
            crate::exec::execute(&arguments, &environment)
        }
        LaunchRequest::Persistent { argv } => {
            let arguments: Vec<_> = argv.into_iter().map(OsString::from_vec).collect();
            crate::detached_child::run(&arguments)
        }
        LaunchRequest::Exec {
            cwd,
            argv,
            handshake,
            new_session,
            inherit_lock,
        } => {
            let mut stream = UnixStream::connect(PathBuf::from(OsString::from_vec(handshake)))?;
            rustix::io::fcntl_setfd(&stream, rustix::io::FdFlags::CLOEXEC)
                .map_err(std::io::Error::from)?;
            let outcome = (|| {
                let lock = if inherit_lock {
                    Some(crate::lock_fd::receive(&mut stream)?)
                } else {
                    None
                };
                if new_session {
                    nix::unistd::setsid().map_err(std::io::Error::from)?;
                }
                crate::detached_child::prepare_with_lock(
                    stream.as_raw_fd(),
                    lock.as_ref().map(AsFd::as_fd),
                )?;
                let environment = crate::exec::environment(None)?;
                std::env::set_current_dir(PathBuf::from(OsString::from_vec(cwd)))?;
                let arguments: Vec<_> = argv.into_iter().map(OsString::from_vec).collect();
                crate::exec::execute(&arguments, &environment)
            })();
            if let Err(error) = outcome {
                let errno = match error {
                    Error::Io(error) => error.raw_os_error().unwrap_or(22),
                    _ => 22,
                };
                stream.write_all(&errno.to_ne_bytes())?;
                std::process::exit(1);
            }
            Ok(())
        }
    }
}

pub(crate) fn spawn_persistent(launcher: &Path, arguments: &[OsString]) -> Result<Child, Error> {
    crate::exec::validate_arguments(arguments)?;
    let request = LaunchRequest::Persistent {
        argv: arguments
            .iter()
            .map(|arg| arg.as_encoded_bytes().into())
            .collect(),
    };
    let mut descriptor = tempfile::NamedTempFile::new()?;
    serde_json::to_writer(descriptor.as_file_mut(), &request)?;
    let mut child = Command::new(launcher)
        .arg(descriptor.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()?;
    let Some(pipe) = child.stdout.take() else {
        child.kill()?;
        child.wait()?;
        return Err(Error::LaunchProtocol("missing error pipe"));
    };
    let mut report = Vec::new();
    if let Err(error) = pipe.take(5).read_to_end(&mut report) {
        child.kill()?;
        child.wait()?;
        return Err(error.into());
    }
    if report.is_empty() {
        return Ok(child);
    }
    if report.len() != 4 {
        child.kill()?;
        child.wait()?;
        return Err(Error::LaunchProtocol("invalid error packet"));
    }
    child.wait()?;
    let bytes = report
        .try_into()
        .map_err(|_| Error::LaunchProtocol("invalid errno width"))?;
    Err(std::io::Error::from_raw_os_error(i32::from_ne_bytes(bytes)).into())
}
