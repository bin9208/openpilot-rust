//! Dynamic comparison of independent Version source policy and a persistent native probe.
//! Mechanical schedules are from check_version_reference.py; captured answers are
//! exclusively test data, never a production input or candidate-policy oracle.
use crate::{path_resolve::resolve_non_strict, seal::Result, version_source::Session};
use openpilot_log_format::{JsonValue as J, JsonView};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs::{self, File},
    io::{BufRead, Write},
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
mod fixtures;
mod json;
pub(crate) mod process;
mod schedule;
use process::{Cancellation, Probe};

#[derive(Clone, Debug)]
pub struct Options {
    pub binary: PathBuf,
    pub runner: Vec<OsString>,
    pub output: PathBuf,
    pub environment: BTreeMap<OsString, OsString>,
}
#[derive(Clone, Debug)]
pub enum Command {
    Source,
    Compare(Options),
}
/// The CLI can restore the original terminating signal only after all owned
/// probes/Git children and IPC resources have been cleaned up by run().
#[derive(Debug)]
pub struct Interrupted(pub i32);
impl std::fmt::Display for Interrupted {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(output, "Version reference interrupted by signal {}", self.0)
    }
}
impl std::error::Error for Interrupted {}
pub fn parse(
    args: &[OsString],
    cwd: &Path,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<Command> {
    if args == [OsString::from("--source")] {
        return Ok(Command::Source);
    }
    let binary = args
        .first()
        .ok_or("Version reference requires BINARY [RUNNER ARGS...]")?;
    let output = environment
        .get(OsStr::new("VERSION_EVIDENCE"))
        .map(OsString::as_os_str)
        .unwrap_or(OsStr::new(".omo/evidence/runtime-version-73/reference"));
    Ok(Command::Compare(Options {
        binary: resolve_non_strict(&cwd.join(binary))?,
        runner: args[1..].to_vec(),
        output: resolve_non_strict(&cwd.join(output))?,
        environment: environment.clone(),
    }))
}
/// The retained --source transport preserves one independent cache/cwd session.
pub fn source(
    cwd: &Path,
    environment: &BTreeMap<OsString, OsString>,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<()> {
    let mut session = Session::new(cwd, environment)?;
    let mut skip_lf = false;
    while let Some(line) = source_line(input, &mut skip_lf)? {
        // The unchanged Linux CPython stdin decodes UTF-8 with surrogateescape
        // before json.loads. Raw undecodable bytes remain Python codepoints.
        let points = crate::python_fs::codepoints(OsStr::from_bytes(&line));
        let response = session.request_codepoints(&points)?;
        writeln!(output, "{}", response.to_json()?)?;
        output.flush()?;
    }
    Ok(())
}
fn source_line(input: &mut dyn BufRead, skip_lf: &mut bool) -> Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    loop {
        let bytes = input.fill_buf()?;
        if bytes.is_empty() {
            return Ok((!line.is_empty()).then_some(line));
        }
        if *skip_lf {
            *skip_lf = false;
            if bytes[0] == b'\n' {
                input.consume(1);
                continue;
            }
        }
        if let Some(end) = bytes.iter().position(|b| matches!(b, b'\r' | b'\n')) {
            line.extend_from_slice(&bytes[..end]);
            *skip_lf = bytes[end] == b'\r';
            input.consume(end + 1);
            return Ok(Some(line));
        }
        let count = bytes.len();
        line.extend_from_slice(bytes);
        input.consume(count);
    }
}
struct Endpoint {
    socket: Option<zmq::Socket>,
    path: PathBuf,
    _identity: tempfile::TempDir,
}
impl Endpoint {
    fn new() -> Result<(Self, OsString)> {
        let identity = tempfile::Builder::new().prefix("version-").tempdir()?;
        let prefix = identity
            .path()
            .file_name()
            .ok_or("Version IPC identity absent")?
            .to_os_string();
        let mut endpoint = OsString::from("/tmp/logmessage");
        endpoint.push(&prefix);
        let path = PathBuf::from(endpoint);
        // This path is derived only from a new private identity, never an old
        // invocation artifact or caller-supplied namespace.
        if fs::symlink_metadata(&path).is_ok() {
            return Err("Version IPC endpoint already present".into());
        }
        let context = zmq::Context::new();
        let socket = context.socket(zmq::PULL)?;
        socket.set_linger(0)?;
        let address = format!(
            "ipc://{}",
            path.to_str().ok_or("Version IPC endpoint not Unicode")?
        );
        socket.bind(&address)?;
        Ok((
            Self {
                socket: Some(socket),
                path,
                _identity: identity,
            },
            prefix,
        ))
    }
    fn poll(&self, duration: Duration, cancel: &Cancellation) -> Result<bool> {
        let end = Instant::now() + duration;
        loop {
            cancel.check()?;
            let left = end.saturating_duration_since(Instant::now());
            let millis = left.min(Duration::from_millis(20)).as_millis() as i64;
            match self
                .socket
                .as_ref()
                .ok_or("Version socket closed")?
                .poll(zmq::POLLIN, millis)
            {
                Ok(events) if events != 0 => return Ok(true),
                Ok(_) => {}
                Err(zmq::Error::EINTR) => continue,
                Err(error) => return Err(error.into()),
            }
            if Instant::now() >= end {
                return Ok(false);
            }
        }
    }
    fn log(&self, expected: &J, cancel: &Cancellation) -> Result<J> {
        if !self.poll(Duration::from_secs(2), cancel)? {
            return Err("Version native log timeout".into());
        }
        let packet = self
            .socket
            .as_ref()
            .ok_or("Version socket closed")?
            .recv_bytes(zmq::DONTWAIT)?;
        let level = expected
            .get("levelnum")
            .ok_or("source log level absent")?
            .to_json()?
            .parse::<u8>()?;
        if packet.first() != Some(&level) {
            return Err("Version native log level byte differs".into());
        }
        let native = J::parse(std::str::from_utf8(&packet[1..])?)?;
        let JsonView::Object(fields) = expected.view() else {
            return Err("source log is not an object".into());
        };
        let mut projection = Vec::new();
        for (key, value) in fields {
            let key = key
                .iter()
                .copied()
                .map(char::from_u32)
                .collect::<Option<String>>()
                .ok_or("invalid source log key")?;
            let actual = native.get(&key).ok_or("Version native log key absent")?;
            // The original log comparator uses ordinary Python equality, not
            // the response-only recursive NaN normalization.
            if !crate::control_tools_policy_check::json::equal(&actual, &value) {
                return Err(format!("Version native log field differs: {key}").into());
            }
            projection.push((key, actual));
        }
        json::object(
            &projection
                .iter()
                .map(|(key, v)| (key.as_str(), v.clone()))
                .collect::<Vec<_>>(),
        )
    }
}
impl Drop for Endpoint {
    fn drop(&mut self) {
        self.socket.take();
        let _ = fs::remove_file(&self.path);
    }
}
struct FixtureRoot(tempfile::TempDir);
impl Drop for FixtureRoot {
    fn drop(&mut self) {
        let path = self.0.path().join("unreadable");
        if path.is_dir() {
            let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
        }
    }
}
struct Pair<'a> {
    source: Session,
    native: Probe,
    endpoint: Endpoint,
    label: &'a str,
    evidence: &'a mut File,
    cancel: &'a Cancellation,
    emulated: bool,
    mismatches: &'a mut Vec<Value>,
    count: usize,
}
impl<'a> Pair<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        options: &Options,
        cwd: &Path,
        environment: &BTreeMap<OsString, OsString>,
        label: &'a str,
        evidence: &'a mut File,
        cancel: &'a Cancellation,
        mismatches: &'a mut Vec<Value>,
    ) -> Result<Self> {
        cancel.check()?;
        let (endpoint, prefix) = Endpoint::new()?;
        let mut env = environment.clone();
        env.insert("OPENPILOT_PREFIX".into(), prefix);
        // Artifacts are opened in original order before probe startup; early
        // exec failures must retain both files and an empty comparisons stream.
        let source_stderr = File::create(options.output.join(format!("{label}-source.stderr")))?;
        let native_stderr = File::create(options.output.join(format!("{label}-native.stderr")))?;
        let mut argv = options.runner.clone();
        argv.push(options.binary.as_os_str().to_owned());
        let mut source = Session::new(cwd, &env)?;
        source.set_stderr(source_stderr);
        let native = Probe::spawn(&argv, cwd, &env, native_stderr)?;
        Ok(Self {
            source,
            native,
            endpoint,
            label,
            evidence,
            cancel,
            emulated: options
                .environment
                .get(OsStr::new("VERSION_EMULATED_SPAWN"))
                .is_some_and(|v| !v.is_empty()),
            mismatches,
            count: 0,
        })
    }
    fn call(&mut self, request: &J) -> Result<()> {
        self.cancel.check()?;
        let request_text = request.to_json()?;
        // Preserve the original source worker's JSON serialization boundary.
        let full = self.source.request_controlled(
            request_text.as_bytes(),
            Instant::now() + Duration::from_secs(10),
            self.cancel.flag(),
        )?;
        let full = J::parse(&full.to_json()?)?;
        self.cancel.check()?;
        let expected = json::without_logs(&full)?;
        let JsonView::Array(logs) = full.get("logs").ok_or("source logs absent")?.view() else {
            return Err("source logs not an array".into());
        };
        self.native
            .write(format!("{request_text}\n").as_bytes(), self.cancel)?;
        let bytes = self.native.line(self.cancel)?;
        let actual = J::parse(std::str::from_utf8(&bytes)?)?;
        let logs = logs
            .iter()
            .map(|log| self.endpoint.log(log, self.cancel))
            .collect::<Result<Vec<_>>>()?;
        let passed = json::normal_equal(&actual, &expected);
        let row = json::object(&[
            ("scenario", J::text(self.label)),
            ("request", request.clone()),
            ("expected", expected.clone()),
            ("actual", actual.clone()),
            ("logs", json::array(&logs)?),
            ("passed", J::parse(if passed { "true" } else { "false" })?),
        ])?;
        let row = row.to_json()?;
        writeln!(self.evidence, "{row}")?;
        self.evidence.flush()?;
        let method = request.get("method");
        let op = request.get("op");
        let limited = self.emulated
            && actual.to_json()? == r#"{"value": ""}"#
            && op.is_some_and(|v| v.text_eq("git"))
            && (self.label == "boundaries"
                && method.as_ref().is_some_and(|v| v.text_eq("commit"))
                && request
                    .get("cwd")
                    .and_then(|v| v.to_utf8())
                    .is_some_and(|p| p.ends_with("/not-yet-present"))
                || self.label == "missing-executable"
                    && method.is_some_and(|v| v.text_eq("origin")));
        if !passed {
            if !limited {
                return Err(format!(
                    "Version response mismatch in {}: {request_text}",
                    self.label
                )
                .into());
            }
            self.mismatches.push(serde_json::from_str(&row)?);
        }
        self.count += 1;
        Ok(())
    }
    fn finish(mut self) -> Result<usize> {
        self.native.finish(self.cancel)?;
        if self
            .endpoint
            .poll(Duration::from_millis(100), self.cancel)?
        {
            return Err(format!("unexpected native Version logs in {}", self.label).into());
        }
        Ok(self.count)
    }
}
pub fn run(repository: &Path, options: &Options) -> Result<J> {
    // A failed new invocation must never leave a prior PASS receipt visible.
    match fs::remove_file(options.output.join("manifest.json")) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let cancel = Cancellation::new()?;
    let result = run_inner(repository, options, &cancel);
    if let Some(signal) = cancel.signal() {
        match fs::remove_file(options.output.join("manifest.json")) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        return Err(Interrupted(signal).into());
    }
    result
}
/// Python-compatible pretty output for the public lossless report value.
pub fn format_report(report: &J) -> Result<String> {
    crate::control_tools_policy_check::json::pretty(report, 0)
}
fn run_inner(repository: &Path, options: &Options, cancel: &Cancellation) -> Result<J> {
    let hashes = crate::version_source::sources(repository)?;
    let historical = crate::version_source::historical_sources(repository)?;
    fs::create_dir_all(&options.output)?;
    let mut evidence = File::create(options.output.join("comparisons.jsonl"))?;
    let root = FixtureRoot(tempfile::Builder::new().prefix("rust-version-").tempdir()?);
    let root = root.0.path();
    let mut counts = BTreeMap::new();
    let mut mismatches = Vec::new();
    for &name in schedule::NAMES {
        cancel.check()?;
        let (cwd, environment) = fixtures::scenario(root, name, &options.environment, cancel)?;
        let mut pair = Pair::new(
            options,
            &cwd,
            &environment,
            name,
            &mut evidence,
            cancel,
            &mut mismatches,
        )?;
        for (index, request) in schedule::requests(root, name)?.iter().enumerate() {
            cancel.check()?;
            fixtures::before_call(root, name, index, &options.environment, cancel)?;
            pair.call(request)?;
        }
        counts.insert(name, pair.finish()?);
    }
    cancel.check()?;
    let comparisons: usize = counts.values().sum();
    let mut invocation = options.runner.clone();
    invocation.push(options.binary.as_os_str().to_owned());
    let invocation = invocation
        .iter()
        .map(|value| crate::python_fs::json_value(value.as_os_str()))
        .collect::<Vec<_>>();
    let binary = crate::python_fs::json_value(options.binary.as_os_str());
    let sources = hashes
        .iter()
        .filter(|(name, _)| {
            [
                "openpilot/system/version.py",
                "openpilot/common/git.py",
                "openpilot/common/utils.py",
            ]
            .contains(&name.as_str())
        })
        .collect::<BTreeMap<_, _>>();
    // The report API and both output streams retain filesystem strings as
    // strings, including lone surrogateescape points unsupported by Value.
    let report = json::object(&[
        (
            "matching_comparisons",
            J::parse(&(comparisons - mismatches.len()).to_string())?,
        ),
        (
            "divergences",
            J::parse(&serde_json::to_string(&mismatches)?)?,
        ),
        ("counts", J::parse(&serde_json::to_string(&counts)?)?),
        ("comparisons", J::parse(&comparisons.to_string())?),
        (
            "historical_source_sha256",
            J::parse(&serde_json::to_string(&historical)?)?,
        ),
        ("binary", binary),
        (
            "binary_sha256",
            J::text(&format!("{:x}", Sha256::digest(fs::read(&options.binary)?))),
        ),
        ("invocation", json::array(&invocation)?),
        (
            "implementation",
            J::text("rust independent source controller"),
        ),
        (
            "source_sha256",
            J::parse(&serde_json::to_string(&sources)?)?,
        ),
        (
            "result",
            J::text(
                if options
                    .environment
                    .get(OsStr::new("VERSION_EMULATED_SPAWN"))
                    .is_some_and(|v| !v.is_empty())
                {
                    "EMULATION_LIMITS"
                } else {
                    "PASS"
                },
            ),
        ),
    ])?;
    // Detect source drift during a run, not merely at admission.
    if crate::version_source::sources(repository)? != hashes
        || crate::version_source::historical_sources(repository)? != historical
    {
        return Err("Version source changed during comparison".into());
    }
    cancel.check()?;
    fs::write(
        options.output.join("manifest.json"),
        format!("{}\n", format_report(&report)?),
    )?;
    Ok(report)
}
