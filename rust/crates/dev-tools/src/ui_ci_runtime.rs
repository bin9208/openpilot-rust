//! Intermediate UI CI infrastructure; Python comparisons and admission gates remain pending.
use crate::{qa_process::Runner, seal::Result};
use std::{
    ffi::{OsStr, OsString},
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Clone, Copy)]
pub struct DisplayPolicy {
    pub startup: Duration,
    pub terminate: Duration,
    pub line_limit: usize,
}
impl Default for DisplayPolicy {
    fn default() -> Self {
        Self {
            startup: Duration::from_secs(15),
            terminate: Duration::from_secs(10),
            line_limit: 64 * 1024,
        }
    }
}
pub struct OwnedDisplay {
    child: std::process::Child,
    display: String,
    policy: DisplayPolicy,
    // Python retains both pipe endpoints throughout the context. In particular,
    // keeping the writer means an early exited child still reaches startup timeout.
    _reader: std::os::unix::net::UnixStream,
    _writer: std::os::unix::net::UnixStream,
    reaped: bool,
}
impl OwnedDisplay {
    pub fn start(program: &OsStr, output: &Path, policy: DisplayPolicy) -> Result<Self> {
        use command_fds::{CommandFdExt, FdMapping};
        use std::{
            fs::File,
            io::Read,
            os::{fd::AsRawFd, unix::net::UnixStream},
            process::{Command, Stdio},
            time::Instant,
        };
        if policy.startup.is_zero() || policy.terminate.is_zero() || policy.line_limit == 0 {
            return Err("display policy must be positive".into());
        }
        let (mut reader, writer) = UnixStream::pair()?;
        let fd = writer.as_raw_fd();
        let argv: Vec<OsString> = vec![
            program.into(),
            "-displayfd".into(),
            fd.to_string().into(),
            "-screen".into(),
            "0".into(),
            "2160x1080x24".into(),
            "-nolisten".into(),
            "tcp".into(),
        ];
        let log = File::create(output.join("xvfb.log"))?;
        let mut command = Command::new(program);
        command
            .args(&argv[1..])
            .stdin(Stdio::inherit())
            .stdout(log.try_clone()?)
            .stderr(log);
        // Parent descriptors stay CLOEXEC. The dependency maps a duplicated OwnedFd
        // only in the child, so unrelated concurrent execs cannot inherit displayfd.
        command.fd_mappings(vec![FdMapping {
            parent_fd: writer.try_clone()?.into(),
            child_fd: fd,
        }])?;
        let retained_reader = reader.try_clone()?;
        let child = command.spawn()?;
        drop(command);
        let mut owner = Self {
            child,
            display: String::new(),
            policy,
            _reader: retained_reader,
            _writer: writer,
            reaped: false,
        };
        let deadline = Instant::now()
            .checked_add(policy.startup)
            .ok_or("display startup deadline overflow")?;
        let mut bytes = Vec::new();
        let number = loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(
                    "Xvfb did not report a complete display within startup deadline".into(),
                );
            }
            reader.set_read_timeout(Some(remaining))?;
            let mut chunk = [0; 8192];
            let size = match reader.read(&mut chunk) {
                Ok(0) => return Err("display descriptor unexpectedly closed".into()),
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    return Err(
                        "Xvfb did not report a complete display within startup deadline".into(),
                    );
                }
                Err(e) => return Err(e.into()),
            };
            bytes.extend_from_slice(&chunk[..size]);
            let text = match std::str::from_utf8(&bytes) {
                Ok(text) => text,
                Err(e) if e.error_len().is_none() => {
                    std::str::from_utf8(&bytes[..e.valid_up_to()])?
                }
                Err(e) => return Err(e.into()),
            };
            if let Some(at) = text.find(['\n', '\r']) {
                if at > policy.line_limit {
                    return Err("display line exceeds policy limit".into());
                }
                break text[..at]
                    .trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
                    .to_owned();
            }
            if bytes.len() > policy.line_limit {
                return Err("display line exceeds policy limit".into());
            }
        };
        if number.is_empty()
            || !crate::argparse_scan::decimal(&number)
            || owner.child.try_wait()?.is_some()
        {
            return Err("Xvfb display failed".into());
        }
        owner.display = format!(":{number}");
        let args = argv
            .iter()
            .map(|v| crate::python_fs::json_string(v))
            .collect::<Result<Vec<_>>>()?
            .join(", ");
        let receipt = format!(
            "{{\"argv\": [{args}], \"pid\": {}, \"display\": {}}}\n",
            owner.pid(),
            crate::python_fs::json_string(OsStr::new(&owner.display))?
        );
        std::fs::write(output.join("display.json"), receipt)?;
        Ok(owner)
    }
    pub fn display(&self) -> &str {
        &self.display
    }
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
    pub fn finish(&mut self) -> Result<()> {
        if self.reaped {
            return Ok(());
        }
        if self.child.try_wait()?.is_some() {
            self.reaped = true;
            return Ok(());
        }
        if let Some(pid) = rustix::process::Pid::from_raw(self.child.id() as i32) {
            match rustix::process::kill_process(pid, rustix::process::Signal::TERM) {
                Ok(()) => {}
                Err(e) if e == rustix::io::Errno::SRCH => {}
                Err(e) => return Err(e.into()),
            }
        }
        let deadline = std::time::Instant::now()
            .checked_add(self.policy.terminate)
            .ok_or("display termination deadline overflow")?;
        loop {
            if self.child.try_wait()?.is_some() {
                self.reaped = true;
                return Ok(());
            }
            if std::time::Instant::now() >= deadline {
                self.child.kill()?;
                self.child.wait()?;
                self.reaped = true;
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
impl Drop for OwnedDisplay {
    fn drop(&mut self) {
        if !self.reaped && self.finish().is_err() {
            let _ = self.child.kill();
            let _ = self.child.wait();
            self.reaped = true;
        }
    }
}
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn fs_text(value: &openpilot_log_format::JsonValue) -> Result<PathBuf> {
    use openpilot_log_format::JsonView;
    use std::os::unix::ffi::OsStringExt;
    let JsonView::Text(points) = value.view() else {
        return Err("expected filesystem string".into());
    };
    let mut bytes = vec![];
    for &p in points {
        if (0xdc80..=0xdcff).contains(&p) {
            bytes.push((p - 0xdc00) as u8)
        } else {
            let c = char::from_u32(p).ok_or("unencodable filesystem surrogate")?;
            let mut utf8 = [0; 4];
            bytes.extend_from_slice(c.encode_utf8(&mut utf8).as_bytes());
        }
    }
    Ok(PathBuf::from(OsString::from_vec(bytes)))
}
pub fn vision_peer(messages: &Path, cwd: &Path) -> Result<PathBuf> {
    use openpilot_log_format::{JsonValue, JsonView};
    let source = text(std::fs::read(messages)?)?;
    let mut outputs = std::collections::BTreeSet::new();
    for line in source.split_terminator([
        '\n', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}', '\u{2029}',
    ]) {
        let record = JsonValue::parse(line)?;
        if !record.is_object() {
            return Err("Cargo record must be object".into());
        }
        if record
            .get("reason")
            .is_some_and(|v| v.text_eq("build-script-executed"))
        {
            let package = record.get("package_id").ok_or("missing package_id")?;
            let contains = match package.view() {
                JsonView::Text(points) => {
                    let needle = "#openpilot-msgq@"
                        .chars()
                        .map(u32::from)
                        .collect::<Vec<_>>();
                    points.windows(needle.len()).any(|p| p == needle)
                }
                JsonView::Array(values) => values.iter().any(|v| v.text_eq("#openpilot-msgq@")),
                JsonView::Object(_) => package.get("#openpilot-msgq@").is_some(),
                _ => return Err("package_id cannot contain text".into()),
            };
            if contains {
                let path = fs_text(&record.get("out_dir").ok_or("missing out_dir")?)?;
                outputs.insert(crate::path_resolve::resolve_non_strict(
                    &if path.is_absolute() {
                        path
                    } else {
                        cwd.join(path)
                    },
                )?);
            }
        }
    }
    if outputs.len() != 1 {
        return Err("expected one executed openpilot-msgq build output".into());
    }
    let peer = outputs.pop_first().unwrap().join("native-vision-peer");
    match std::fs::metadata(&peer) {
        Ok(m) if m.is_file() => Ok(peer),
        Ok(_) => Err("vision peer is not a file".into()),
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            Err("vision peer is not a file".into())
        }
        Err(e) => Err(e.into()),
    }
}
#[allow(clippy::too_many_arguments)]
pub fn run_evidence(
    argv: &[OsString],
    cwd: &Path,
    evidence: &Path,
    name: &str,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
    errors: &mut dyn Write,
    timeout: Duration,
) -> Result<()> {
    std::fs::create_dir_all(evidence)?;
    let items = argv
        .iter()
        .map(|v| crate::python_fs::json_string(v))
        .collect::<Result<Vec<_>>>()?;
    let args = if items.is_empty() {
        "[]".into()
    } else {
        format!("[\n    {}\n  ]", items.join(",\n    "))
    };
    let receipt = format!(
        "{{\n  \"argv\": {args},\n  \"cwd\": {}\n}}\n",
        crate::python_fs::json_string(cwd.as_os_str())?
    );
    std::fs::write(evidence.join(format!("{name}.command.json")), receipt)?;
    let mut spec = crate::qa_process::CommandSpec::new_os(argv.to_vec(), timeout);
    spec.cwd = Some(cwd.into());
    spec.inherit_stdin = true;
    spec.stdout_limit = 64 * 1024 * 1024;
    spec.stderr_limit = 64 * 1024 * 1024;
    let captured = runner.run(&spec)?;
    let stdout = text(captured.stdout)?;
    let stderr = text(captured.stderr)?;
    std::fs::write(evidence.join(format!("{name}.stdout")), &stdout)?;
    std::fs::write(evidence.join(format!("{name}.stderr")), &stderr)?;
    std::fs::write(
        evidence.join(format!("{name}.exit.json")),
        format!("{{\"returncode\": {}}}\n", captured.code),
    )?;
    if captured.code != 0 {
        writeln!(errors, "{stdout}{stderr}")?;
        return Err(format!("command {name} exited {}", captured.code).into());
    }
    writeln!(printed, "PASS {name}")?;
    printed.flush()?;
    Ok(())
}
