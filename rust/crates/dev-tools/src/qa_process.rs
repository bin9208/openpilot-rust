//! Bounded native QA command capture with deadlines and owned process groups.
//! Cleanup sends SIGKILL to the owned group and reaps its direct leader. Group
//! descendants terminate asynchronously; this owner does not reap non-children.
use crate::seal::Result;
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
    process::{kill_process_group, waitid, Pid, Signal, WaitId, WaitIdOptions},
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fmt,
    fs::File,
    io::{self, Read, Write},
    os::{
        fd::OwnedFd,
        unix::{
            net::UnixStream,
            process::{CommandExt, ExitStatusExt},
        },
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
#[derive(Clone, Debug)]
pub struct CommandSpec {
    pub argv: Vec<String>,
    /// Authoritative Unix argv when supplied. The String argv remains a UTF-8
    /// peer interface; new_os leaves it empty if any argument is not Unicode.
    pub os_argv: Option<Vec<OsString>>,
    /// Override the child's argv[0] while executing the program selected by
    /// effective_argv. Some native compilers use this spelling to find helpers.
    pub arg0: Option<OsString>,
    pub cwd: Option<PathBuf>,
    pub env: BTreeMap<String, String>,
    /// Authoritative environment overlay, preserving native OS bytes. The
    /// caller's environment is still inherited; Some(empty) adds no variables.
    pub os_env: Option<BTreeMap<OsString, OsString>>,
    /// Remove these inherited or overlaid variables from the child environment.
    /// Applied after both environment overlays; the parent is unchanged.
    pub env_remove: Vec<OsString>,
    pub timeout: Duration,
    pub stdout_limit: usize,
    pub stderr_limit: usize,
    /// Use one child output stream, preserving stdout/stderr write order.
    /// Merged bytes are returned in stdout; stderr is empty.
    pub merge_output: bool,
    /// Pass both output streams directly to the caller without capturing them.
    pub inherit_output: bool,
    /// Inherit only stderr, retaining captured stdout (e.g. Cargo JSON output).
    pub inherit_stderr: bool,
    /// Feed these raw bytes through a pipe while concurrently draining outputs.
    /// None preserves the existing null-stdin behavior; Some(empty) sends EOF.
    pub input: Option<Vec<u8>>,
    /// Pass the caller's stdin through unchanged; incompatible with explicit input.
    pub inherit_stdin: bool,
    /// Disable core files in this child only, matching source oracle pre-exec policy.
    pub disable_core_dump: bool,
}
impl CommandSpec {
    pub fn new(argv: Vec<String>, timeout: Duration) -> Self {
        Self {
            argv,
            os_argv: None,
            arg0: None,
            cwd: None,
            env: BTreeMap::new(),
            os_env: None,
            env_remove: Vec::new(),
            timeout,
            stdout_limit: 16 * 1024 * 1024,
            stderr_limit: 16 * 1024 * 1024,
            merge_output: false,
            inherit_output: false,
            inherit_stderr: false,
            input: None,
            inherit_stdin: false,
            disable_core_dump: false,
        }
    }
    pub fn new_os(argv: Vec<OsString>, timeout: Duration) -> Self {
        let mirror = argv
            .iter()
            .cloned()
            .map(OsString::into_string)
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap_or_default();
        let mut spec = Self::new(mirror, timeout);
        spec.os_argv = Some(argv);
        spec
    }
    pub fn effective_argv(&self) -> Vec<OsString> {
        self.os_argv
            .clone()
            .unwrap_or_else(|| self.argv.iter().map(OsString::from).collect())
    }
    pub fn child_argv(&self) -> Vec<OsString> {
        let mut argv = self.effective_argv();
        if let (Some(first), Some(arg0)) = (argv.first_mut(), &self.arg0) {
            first.clone_from(arg0);
        }
        argv
    }
    pub fn set_os_env(&mut self, env: BTreeMap<OsString, OsString>) {
        self.env = env
            .iter()
            .map(|(key, value)| Some((key.to_str()?.to_owned(), value.to_str()?.to_owned())))
            .collect::<Option<BTreeMap<_, _>>>()
            .unwrap_or_default();
        self.os_env = Some(env);
    }
    pub fn effective_env(&self) -> BTreeMap<OsString, OsString> {
        self.os_env.clone().unwrap_or_else(|| {
            self.env
                .iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect()
        })
    }
}
#[derive(Clone, Debug)]
pub struct Captured {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}
/// A separate output file could not be opened before the command was spawned.
#[derive(Debug)]
pub struct OutputFileError {
    pub path: PathBuf,
    pub error: io::Error,
}
impl fmt::Display for OutputFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.error)
    }
}
impl std::error::Error for OutputFileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
fn output_file(path: &Path) -> Result<File> {
    File::create(path).map_err(|error| {
        OutputFileError {
            path: path.into(),
            error,
        }
        .into()
    })
}
pub trait Runner {
    fn run(&mut self, command: &CommandSpec) -> Result<Captured>;
    /// Stream separate raw outputs to owned files. Native execution returns
    /// status with empty buffers; controlled peers retain their captured bytes.
    /// Logs are created in stdout/stderr order before attempting child startup.
    fn run_to_files(
        &mut self,
        command: &CommandSpec,
        stdout: &Path,
        stderr: &Path,
    ) -> Result<Captured> {
        let mut stdout = output_file(stdout)?;
        let mut stderr = output_file(stderr)?;
        let mut command = command.clone();
        command.merge_output = false;
        command.inherit_output = false;
        command.inherit_stderr = false;
        match self.run(&command) {
            Ok(captured) => {
                stdout.write_all(&captured.stdout)?;
                stderr.write_all(&captured.stderr)?;
                Ok(captured)
            }
            Err(error) => {
                if let Some(CaptureError::Timeout {
                    stdout: out,
                    stderr: err,
                    ..
                }) = error.downcast_ref::<CaptureError>()
                {
                    stdout.write_all(out)?;
                    stderr.write_all(err)?;
                }
                Err(error)
            }
        }
    }
    /// Merge both streams into an owned file. Test peers can use captured bytes;
    /// NativeRunner redirects directly, avoiding an in-memory diagnostic limit.
    fn run_to_file(&mut self, command: &CommandSpec, log: &Path) -> Result<Captured> {
        self.run_to_file_with_prefix(command, log, &[])
    }
    /// Write and flush invocation provenance before running a child. Retain the
    /// prefix on spawn failure or timeout, along with any partial diagnostics.
    fn run_to_file_with_prefix(
        &mut self,
        command: &CommandSpec,
        log: &Path,
        prefix: &[u8],
    ) -> Result<Captured> {
        let mut log = File::create(log)?;
        log.write_all(prefix)?;
        log.flush()?;
        let mut command = command.clone();
        command.merge_output = true;
        command.inherit_output = false;
        command.inherit_stderr = false;
        match self.run(&command) {
            Ok(captured) => {
                log.write_all(&captured.stdout)?;
                Ok(captured)
            }
            Err(error) => {
                if let Some(CaptureError::Timeout { stdout, .. }) =
                    error.downcast_ref::<CaptureError>()
                {
                    log.write_all(stdout)?;
                }
                Err(error)
            }
        }
    }
}
#[derive(Debug)]
pub enum CaptureError {
    Timeout {
        timeout: Duration,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
    Limit {
        stream: &'static str,
        limit: usize,
    },
}
impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout { timeout, .. } => {
                write!(f, "native command timed out after {timeout:?}")
            }
            Self::Limit { stream, limit } => {
                write!(f, "native command {stream} exceeds {limit} captured bytes")
            }
        }
    }
}
impl std::error::Error for CaptureError {}
struct Owned {
    child: Child,
    pid: Pid,
    reaped: bool,
    own_group: bool,
}
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.reaped {
            if self.own_group {
                let _ = kill_process_group(self.pid, Signal::KILL);
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
struct Pipe {
    file: File,
    bytes: Vec<u8>,
    eof: bool,
}
impl Pipe {
    fn new(fd: OwnedFd) -> Result<Self> {
        let file = File::from(fd);
        fcntl_setfl(&file, fcntl_getfl(&file)? | OFlags::NONBLOCK)?;
        Ok(Self {
            file,
            bytes: Vec::new(),
            eof: false,
        })
    }
    fn drain(&mut self, limit: usize, name: &'static str) -> Result<()> {
        if self.eof {
            return Ok(());
        }
        let mut buffer = [0; 32768];
        match self.file.read(&mut buffer) {
            Ok(0) => self.eof = true,
            Ok(n) => {
                if n > limit.saturating_sub(self.bytes.len()) {
                    return Err(CaptureError::Limit {
                        stream: name,
                        limit,
                    }
                    .into());
                }
                self.bytes.extend_from_slice(&buffer[..n]);
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) => {}
            Err(e) => return Err(e.into()),
        }
        Ok(())
    }
}
pub struct NativeRunner;
enum LogFiles<'a> {
    Merged(&'a Path, &'a [u8]),
    Separate(&'a Path, &'a Path),
}
impl Runner for NativeRunner {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.run_with_file(spec, None, true)
    }
    fn run_to_files(
        &mut self,
        spec: &CommandSpec,
        stdout: &Path,
        stderr: &Path,
    ) -> Result<Captured> {
        let mut spec = spec.clone();
        spec.merge_output = false;
        spec.inherit_output = false;
        spec.inherit_stderr = false;
        self.run_with_file(&spec, Some(LogFiles::Separate(stdout, stderr)), true)
    }
    fn run_to_file_with_prefix(
        &mut self,
        spec: &CommandSpec,
        log: &Path,
        prefix: &[u8],
    ) -> Result<Captured> {
        let mut spec = spec.clone();
        spec.merge_output = true;
        spec.inherit_output = false;
        spec.inherit_stderr = false;
        self.run_with_file(&spec, Some(LogFiles::Merged(log, prefix)), true)
    }
}
impl NativeRunner {
    /// Use one caller-owned deadline across several child commands. Cancellation
    /// follows the same owned-group cleanup as capture failures and timeouts.
    pub fn run_controlled(
        &mut self,
        spec: &CommandSpec,
        deadline: Instant,
        cancelled: &AtomicBool,
    ) -> Result<Captured> {
        self.capture(spec, None, true, Some((deadline, cancelled)), None)
    }
    #[cfg(feature = "version-tools-native")]
    pub(crate) fn run_with_stderr(
        &mut self,
        spec: &CommandSpec,
        control: Option<(Instant, &AtomicBool)>,
        stderr: &File,
    ) -> Result<Captured> {
        self.capture(spec, None, true, control, Some(stderr))
    }
    /// Capture inside an already-owned worker group. The outer runner alone
    /// owns descendants; this inner owner only kills/reaps its direct child.
    /// Never use this entrypoint outside that process-group ownership scope.
    #[cfg(any(
        feature = "timed-live-native",
        feature = "version-tools-native",
        feature = "soundd-daemon-native",
        feature = "bridge-tools-native"
    ))]
    pub(crate) fn run_in_current_group(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.run_with_file(spec, None, false)
    }
    fn run_with_file(
        &mut self,
        spec: &CommandSpec,
        log: Option<LogFiles<'_>>,
        own_group: bool,
    ) -> Result<Captured> {
        self.capture(spec, log, own_group, None, None)
    }
    fn capture(
        &mut self,
        spec: &CommandSpec,
        log: Option<LogFiles<'_>>,
        own_group: bool,
        control: Option<(Instant, &AtomicBool)>,
        stderr_file: Option<&File>,
    ) -> Result<Captured> {
        if let Some((deadline, cancelled)) = control {
            if cancelled.load(Ordering::Relaxed) {
                return Err(io::Error::from(io::ErrorKind::Interrupted).into());
            }
            if Instant::now() >= deadline {
                return Err(CaptureError::Timeout {
                    timeout: Duration::ZERO,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                }
                .into());
            }
        }
        if spec.timeout.is_zero() {
            return Err("native command timeout must be positive".into());
        }
        if spec.merge_output && (spec.inherit_output || spec.inherit_stderr) {
            return Err("native command cannot both merge and inherit output".into());
        }
        if spec.inherit_stdin && spec.input.is_some() {
            return Err("native command cannot both pipe and inherit stdin".into());
        }
        let argv = spec.effective_argv();
        let (first, args) = argv.split_first().ok_or("empty native command")?;
        let mut command = Command::new(first);
        if let Some(arg0) = &spec.arg0 {
            command.arg0(arg0);
        }
        command
            .args(args)
            .envs(spec.effective_env())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if own_group {
            command.process_group(0);
        }
        for key in &spec.env_remove {
            command.env_remove(key);
        }
        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }
        if spec.input.is_some() {
            command.stdin(Stdio::piped());
        } else if spec.inherit_stdin {
            command.stdin(Stdio::inherit());
        }
        if spec.inherit_output {
            command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
        } else if spec.inherit_stderr {
            command.stderr(Stdio::inherit());
        }
        if let Some(file) = stderr_file {
            command.stderr(Stdio::from(file.try_clone()?));
        }
        let file_backed = log.is_some();
        let merged = if let Some(LogFiles::Merged(log, prefix)) = log {
            let mut file = File::create(log)?;
            file.write_all(prefix)?;
            file.flush()?;
            command
                .stdout(Stdio::from(file.try_clone()?))
                .stderr(Stdio::from(file));
            None
        } else if let Some(LogFiles::Separate(stdout, stderr)) = log {
            let stdout = output_file(stdout)?;
            let stderr = output_file(stderr)?;
            command
                .stdout(Stdio::from(stdout))
                .stderr(Stdio::from(stderr));
            None
        } else if spec.merge_output {
            let (reader, writer) = UnixStream::pair()?;
            command
                .stdout(Stdio::from(OwnedFd::from(writer.try_clone()?)))
                .stderr(Stdio::from(OwnedFd::from(writer)));
            Some(OwnedFd::from(reader))
        } else {
            None
        };
        if spec.disable_core_dump {
            openpilot_process_limits::disable_core_dump(&mut command);
        }
        let child = command.spawn()?;
        // Command retains supplied Stdio handles for reuse. Keeping either
        // merged writer here would prevent EOF after the child exits.
        drop(command);
        let pid = Pid::from_raw(i32::try_from(child.id())?).ok_or("invalid owned child PID")?;
        let mut owned = Owned {
            child,
            pid,
            reaped: false,
            own_group,
        };
        let mut input = owned.child.stdin.take();
        if let Some(pipe) = &input {
            fcntl_setfl(pipe, fcntl_getfl(pipe)? | OFlags::NONBLOCK)?;
        }
        let input_bytes = spec.input.as_deref().unwrap_or_default();
        let mut input_written = 0;
        let stdout_fd = match merged {
            Some(fd) => fd,
            None if spec.inherit_output || file_backed => File::open("/dev/null")?.into(),
            None => owned
                .child
                .stdout
                .take()
                .ok_or("missing native stdout")?
                .into(),
        };
        let stderr_fd = if spec.merge_output
            || spec.inherit_output
            || spec.inherit_stderr
            || file_backed
            || stderr_file.is_some()
        {
            File::open("/dev/null")?.into()
        } else {
            owned
                .child
                .stderr
                .take()
                .ok_or("missing native stderr")?
                .into()
        };
        let mut stdout = Pipe::new(stdout_fd)?;
        let mut stderr = Pipe::new(stderr_fd)?;
        let started = Instant::now();
        let timeout = control.map_or(spec.timeout, |(deadline, _)| {
            spec.timeout
                .min(deadline.saturating_duration_since(started))
        });
        let mut exited = false;
        loop {
            if control.is_some_and(|(_, cancelled)| cancelled.load(Ordering::Relaxed)) {
                return Err(io::Error::from(io::ErrorKind::Interrupted).into());
            }
            let Some(remaining) = timeout
                .checked_sub(started.elapsed())
                .filter(|d| !d.is_zero())
            else {
                return Err(CaptureError::Timeout {
                    timeout,
                    stdout: stdout.bytes,
                    stderr: stderr.bytes,
                }
                .into());
            };
            stdout.drain(spec.stdout_limit, "stdout")?;
            stderr.drain(spec.stderr_limit, "stderr")?;
            // One bounded write per iteration prevents a continuously reading
            // child from starving output drainage or the deadline check.
            if let Some(pipe) = &mut input {
                if input_written < input_bytes.len() {
                    let end = input_bytes.len().min(input_written.saturating_add(65536));
                    match pipe.write(&input_bytes[input_written..end]) {
                        Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero).into()),
                        Ok(count) => input_written += count,
                        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => {
                            // subprocess.communicate likewise tolerates early
                            // closure and returns the child's actual status.
                            input_written = input_bytes.len();
                        }
                        Err(error)
                            if matches!(
                                error.kind(),
                                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                            ) => {}
                        Err(error) => return Err(error.into()),
                    }
                }
                if input_written == input_bytes.len() {
                    input = None;
                }
            }
            if !exited {
                match waitid(
                    WaitId::Pid(pid),
                    WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
                ) {
                    Ok(Some(_)) => {
                        exited = true;
                        if own_group {
                            match kill_process_group(pid, Signal::KILL) {
                                Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                                Err(e) => return Err(e.into()),
                            }
                        }
                    }
                    Ok(None) | Err(rustix::io::Errno::INTR) => {}
                    Err(e) => return Err(e.into()),
                }
            }
            if exited && stdout.eof && stderr.eof {
                let status = owned.child.wait()?;
                owned.reaped = true;
                return Ok(Captured {
                    code: status
                        .code()
                        .unwrap_or_else(|| -status.signal().unwrap_or(1)),
                    stdout: stdout.bytes,
                    stderr: stderr.bytes,
                });
            }
            let mut fds = Vec::new();
            if !stdout.eof {
                fds.push(PollFd::new(&stdout.file, PollFlags::IN));
            }
            if !stderr.eof {
                fds.push(PollFd::new(&stderr.file, PollFlags::IN));
            }
            if let Some(pipe) = &input {
                fds.push(PollFd::new(pipe, PollFlags::OUT));
            }
            let timeout = Timespec::try_from(remaining.min(Duration::from_millis(20)))?;
            match poll(&mut fds, Some(&timeout)) {
                Ok(_) | Err(rustix::io::Errno::INTR) => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
}
