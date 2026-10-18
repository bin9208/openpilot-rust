//! Dynamic comparisons against the independently ported original tombstoned Worker.
//! Captured answers are test-only: every expected policy response below is executed
//! by tombstone_source, while the candidate is a separate native process.
use crate::{
    path_resolve::resolve_non_strict,
    seal::{digest, Result},
    tombstone_source::Session,
};
use openpilot_log_format::{JsonValue as J, JsonView};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
mod files;
mod json;
mod schedule;
#[cfg(test)]
mod tests;
pub use crate::journald_check::Random;
use crate::version_reference_check::process::{Cancellation, Probe};
use json::{array, equal, normalize, object, scalar};

#[derive(Clone, Debug)]
pub struct Options {
    pub binary: PathBuf,
    /// The original unchanged Params binding is provenance, never executed.
    pub params_binding: PathBuf,
    pub output: PathBuf,
    pub runner: Vec<OsString>,
    pub environment: BTreeMap<OsString, OsString>,
}
#[derive(Clone, Debug)]
pub enum Command {
    Source,
    Compare(Options),
}
pub use crate::version_reference_check::Interrupted;
pub fn parse(
    args: &[OsString],
    cwd: &Path,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<Command> {
    if args == [OsString::from("--source")] {
        return Ok(Command::Source);
    }
    if args.len() < 3 {
        return Err(
            "tombstone reference requires BINARY PARAMS_BINDING OUTPUT [RUNNER ARGS...]".into(),
        );
    }
    Ok(Command::Compare(Options {
        binary: resolve_non_strict(&cwd.join(&args[0]))?,
        params_binding: resolve_non_strict(&cwd.join(&args[1]))?,
        output: resolve_non_strict(&cwd.join(&args[2]))?,
        runner: args[3..].to_vec(),
        environment: environment.clone(),
    }))
}
pub fn source(
    cwd: &Path,
    environment: &BTreeMap<OsString, OsString>,
    input: &mut dyn std::io::BufRead,
    output: &mut dyn Write,
) -> Result<()> {
    Ok(crate::tombstone_source::source(
        cwd,
        environment,
        input,
        output,
    )?)
}
pub fn format_report(report: &J) -> Result<String> {
    crate::control_tools_policy_check::json::pretty(report, 0)
}
fn field(value: &J, key: &str) -> Result<J> {
    value
        .get(key)
        .ok_or_else(|| format!("missing tombstone field {key}").into())
}
fn elements(value: &J) -> Result<Vec<J>> {
    let JsonView::Array(values) = value.view() else {
        return Err("expected tombstone JSON array".into());
    };
    Ok(values)
}
fn packet_json(packet: &[u8]) -> Result<J> {
    Ok(J::parse(std::str::from_utf8(packet)?)?)
}

struct Endpoint {
    socket: Option<zmq::Socket>,
    _context: zmq::Context,
    path: PathBuf,
    prefix: String,
}
impl Endpoint {
    fn new(token: &Path) -> Result<Self> {
        let leaf = token
            .file_name()
            .and_then(OsStr::to_str)
            .ok_or("invalid private tombstone root")?;
        let prefix = format!("tombstone-{leaf}");
        let path = PathBuf::from(format!("/tmp/logmessage{prefix}"));
        let context = zmq::Context::new();
        let socket = context.socket(zmq::PULL)?;
        socket.set_linger(0)?;
        socket.set_rcvtimeo(100)?;
        socket.bind(&format!(
            "ipc://{}",
            path.to_str().ok_or("non-UTF8 private endpoint")?
        ))?;
        Ok(Self {
            socket: Some(socket),
            _context: context,
            path,
            prefix,
        })
    }
    fn records(&self, index: usize, minimum: usize, cancel: &Cancellation) -> Result<Vec<J>> {
        let marker = J::text(&format!("barrier-{index}"));
        let mut barrier = false;
        let mut records = Vec::new();
        let end = Instant::now() + Duration::from_secs(40);
        while !barrier || records.len() < minimum {
            cancel.check()?;
            if Instant::now() >= end {
                return Err("tombstone log barrier exceeded 40s".into());
            }
            let packet = match self
                .socket
                .as_ref()
                .ok_or("log socket closed")?
                .recv_bytes(0)
            {
                Ok(v) => v,
                Err(zmq::Error::EAGAIN | zmq::Error::EINTR) => continue,
                Err(e) => return Err(e.into()),
            };
            let (&level, raw) = packet.split_first().ok_or("empty tombstone log packet")?;
            let record = packet_json(raw)?;
            if field(&record, "levelnum")?.to_json()?.parse::<u8>()? != level {
                return Err("tombstone log level byte differs from record".into());
            }
            if equal(&field(&record, "msg")?, &marker) {
                barrier = true;
            } else {
                records.push(record);
            }
        }
        Ok(records)
    }
}
impl Drop for Endpoint {
    fn drop(&mut self) {
        self.socket.take();
        let _ = fs::remove_file(&self.path);
    }
}

/// Shared dynamic source/native pair for tombstone and external SDK controllers.
/// The source record remains accessible before any normalization so SDK value_json
/// payloads can independently be sent to the pinned real SDK policy boundary.
pub(crate) struct Pair<'a> {
    root: files::Root,
    output: PathBuf,
    source: Option<Session>,
    native: Option<Probe>,
    endpoint: Endpoint,
    cancel: &'a Cancellation,
    index: usize,
    native_index: usize,
    rows: Vec<J>,
    #[cfg(feature = "sdk-transport-native")]
    last_source: Option<J>,
    emulated: bool,
}
impl<'a> Pair<'a> {
    pub(crate) fn new(
        repository: &Path,
        options: &Options,
        output: &Path,
        cancel: &'a Cancellation,
    ) -> Result<Self> {
        sources(repository)?;
        fs::create_dir_all(output)?;
        match fs::remove_file(output.join("comparisons.jsonl")) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let root = files::Root(
            tempfile::Builder::new()
                .prefix("tombstone-oracle-")
                .tempdir()?,
        );
        let endpoint = Endpoint::new(root.0.path())?;
        let helper = fs::canonicalize(
            repository.join("rust/crates/dev-tools/src/tombstone_reference_check/apport-retrace"),
        )?;
        if digest(&helper)? != "13d137a3f8184726b117056c21b15dc51ef2e89677ac397b838b2fbae294d6fe"
            || fs::metadata(&helper)?.permissions().mode() & 0o111 == 0
        {
            return Err("changed tombstone mechanical retrace helper".into());
        }
        let mut environments = Vec::new();
        for kind in ["source", "native"] {
            let directory = root.0.path().join(kind);
            for folder in [
                "apport",
                "logs",
                "base/openpilot/common",
                "params/fixture",
                "bin",
            ] {
                fs::create_dir_all(directory.join(folder))?;
            }
            files::write(
                &directory,
                Path::new("base/openpilot/common/version.h"),
                b"#define VERSION \"fixture-1.2\"\n",
                None,
            )?;
            files::write(&directory,Path::new("base/build.json"),b"{\"channel\":\"nightly\",\"openpilot\":{\"git_origin\":\"git@github.com:commaai/openpilot.git\",\"git_commit\":\"abcdefghijk\",\"version\":\"metadata-version\"}}",None)?;
            files::write(
                &directory,
                Path::new("params/fixture/DongleId"),
                b"fixture-device",
                None,
            )?;
            files::write(
                &directory,
                Path::new("trace"),
                schedule::TRACE.as_bytes(),
                None,
            )?;
            files::write(&directory, Path::new("mode"), b"normal", None)?;
            symlink(&helper, directory.join("bin/apport-retrace"))?;
            let mut env = options.environment.clone();
            env.insert("OPENPILOT_PREFIX".into(), endpoint.prefix.clone().into());
            let mut path = directory.join("bin").into_os_string();
            path.push(":/usr/bin:/bin");
            env.insert("PATH".into(), path);
            for (key, name) in [
                ("FIXTURE_INPUT", "retrace.input"),
                ("FIXTURE_MODE", "mode"),
                ("FIXTURE_TRACE", "trace"),
                ("FIXTURE_PID", "helper.pid"),
            ] {
                env.insert(key.into(), directory.join(name).into_os_string());
            }
            env.insert("PYTHONDONTWRITEBYTECODE".into(), "1".into());
            env.insert(
                "TOMBSTONE_REFERENCE_OWNER".into(),
                root.0.path().as_os_str().to_owned(),
            );
            environments.push(env);
        }
        let mut source = Session::new(&root.0.path().join("source"), &environments[0])?;
        source.set_stderr(File::create(output.join("source.stderr"))?)?;
        let mut argv = options.runner.clone();
        argv.push(options.binary.as_os_str().to_owned());
        let native = Probe::spawn(
            &argv,
            &root.0.path().join("native"),
            &environments[1],
            File::create(output.join("native.stderr"))?,
        )?;
        let mut pair = Self {
            root,
            output: output.to_owned(),
            source: Some(source),
            native: Some(native),
            endpoint,
            cancel,
            index: 0,
            native_index: 0,
            rows: Vec::new(),
            #[cfg(feature = "sdk-transport-native")]
            last_source: None,
            emulated: options
                .environment
                .get(OsStr::new("TOMBSTONED_EMULATED_SPAWN"))
                .is_some_and(|v| !v.is_empty()),
        };
        pair.call(&scalar("{\"op\":\"configure\",\"base\":\"$FIXTURE/base\",\"params\":\"$FIXTURE/params\",\"pc\":false,\"device\":\"tici\"}"),false)?;
        Ok(pair)
    }
    pub(crate) fn root(&self) -> &Path {
        self.root.0.path()
    }
    pub(crate) fn paths(&self, relative: &Path) -> [PathBuf; 2] {
        [
            self.root().join("source").join(relative),
            self.root().join("native").join(relative),
        ]
    }
    pub(crate) fn write(&self, relative: &Path, bytes: &[u8], mode: Option<u32>) -> Result<()> {
        for side in ["source", "native"] {
            files::write(&self.root().join(side), relative, bytes, mode)?;
        }
        Ok(())
    }
    #[cfg(feature = "sdk-transport-native")]
    pub(crate) fn last_source_record(&self) -> Option<&J> {
        self.last_source.as_ref()
    }
    fn native_response(&mut self, request: &J, end: Instant) -> Result<J> {
        let raw = json::expand(request, &self.root().join("native"))?.to_json()? + "\n";
        let native = self.native.as_mut().ok_or("tombstone probe closed")?;
        native.write_until(raw.as_bytes(), self.cancel, end)?;
        packet_json(&native.line_until(self.cancel, end)?)
    }
    #[cfg(feature = "sdk-transport-native")]
    pub(crate) fn raw_native(&mut self, request: &J) -> Result<(J, Vec<J>)> {
        let actual = self.native_response(request, Instant::now() + Duration::from_secs(40))?;
        let logs = self.endpoint.records(self.native_index, 0, self.cancel)?;
        self.native_index += 1;
        Ok((actual, logs))
    }
    pub(crate) fn call(&mut self, request: &J, compare_files: bool) -> Result<J> {
        self.cancel.check()?;
        let raw = json::expand(request, &self.root().join("source"))?.to_json()?;
        let end = Instant::now() + Duration::from_secs(40);
        // Neither side gets a head start: in particular two 30s retrace requests
        // share one response deadline, matching the original two-worker pipes.
        let mut source = self.source.take().ok_or("tombstone source closed")?;
        let flag = self.cancel.flag();
        let (expected, actual) = std::thread::scope(|scope| {
            let worker =
                scope.spawn(|| source.request_controlled(raw.as_bytes(), end, flag.clone()));
            let actual = self.native_response(request, end);
            if actual.is_err() {
                flag.store(true, Ordering::Relaxed);
            }
            let expected = worker
                .join()
                .map_err(|_| "tombstone source worker panicked");
            (expected, actual)
        });
        self.source = Some(source);
        let source_record = expected.map_err(|e| -> Box<dyn std::error::Error> { e.into() })??;
        let actual = actual?;
        let expected_logs = field(&source_record, "logs")?;
        let expected = json::without_logs(&source_record)?;
        #[cfg(feature = "sdk-transport-native")]
        {
            self.last_source = Some(source_record);
        }
        let records = self.endpoint.records(
            self.native_index,
            elements(&expected_logs)?.len(),
            self.cancel,
        )?;
        self.native_index += 1;
        let projected = records
            .iter()
            .map(|record| {
                object(&[
                    ("msg", field(record, "msg")?),
                    ("levelnum", field(record, "levelnum")?),
                    (
                        "exception",
                        scalar(if record.get("exc_info").is_some() {
                            "true"
                        } else {
                            "false"
                        }),
                    ),
                    ("created", field(record, "created")?),
                ])
            })
            .collect::<Result<Vec<_>>>()?;
        let source_value = normalize(&expected, self.root())?;
        let native_value = normalize(&actual, self.root())?;
        let mut source_logs = normalize(&expected_logs, self.root())?;
        let mut native_logs = normalize(&array(&projected)?, self.root())?;
        if field(request, "op")?.text_eq("cycle") {
            source_logs = json::cycle_logs(&source_logs)?;
            native_logs = json::cycle_logs(&native_logs)?;
        }
        let mut passed = equal(&source_value, &native_value) && equal(&source_logs, &native_logs);
        let snapshots = if compare_files {
            let values = [
                files::snapshot(&self.root().join("source"))?,
                files::snapshot(&self.root().join("native"))?,
            ];
            passed &= equal(&values[0], &values[1]);
            array(&values)?
        } else {
            scalar("null")
        };
        let row = object(&[
            ("index", J::parse(&self.index.to_string())?),
            ("request", request.clone()),
            ("expected", expected),
            ("actual", actual.clone()),
            ("source_logs", expected_logs),
            ("native_logs", array(&records)?),
            ("files", snapshots),
            ("passed", scalar(if passed { "true" } else { "false" })),
        ])?;
        let mut stream = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.output.join("comparisons.jsonl"))?;
        writeln!(stream, "{}", row.to_json()?)?;
        stream.flush()?;
        self.rows.push(row);
        let emulation_limit = self.emulated
            && request
                .get("shell")
                .is_some_and(|v| v.text_eq("$FIXTURE/missing-bash"))
            && equal(
                &source_value,
                &scalar("{\"result\":{\"error\":\"FileNotFoundError\"},\"calls\":[]}"),
            )
            && equal(
                &native_value,
                &scalar("{\"result\":{\"value\":\"Error getting stacktrace\"},\"calls\":[]}"),
            )
            && equal(&source_logs, &scalar("[]"))
            && equal(&native_logs, &scalar("[]"));
        if !passed && !emulation_limit {
            return Err(format!(
                "tombstone comparison {} differs: {}",
                self.index,
                request.to_json()?
            )
            .into());
        }
        self.index += 1;
        Ok(actual)
    }
    pub(crate) fn finish(mut self) -> Result<usize> {
        self.stop_helpers()?;
        if let Some(native) = self.native.as_mut() {
            native.finish(self.cancel)?;
        }
        self.native.take();
        self.source.take();
        files::cleanup_tagged(self.root())?;
        Ok(self.index)
    }
    fn stop_helpers(&self) -> Result<()> {
        files::stop_helpers(&self.paths(Path::new("helper.pid")))
    }
}
impl Drop for Pair<'_> {
    fn drop(&mut self) {
        self.native.take();
        self.source.take();
        let _ = files::cleanup_tagged(self.root());
    }
}

pub(crate) fn sources(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut source = crate::tombstone_source::sources(root)?;
    let name = "openpilot/common/params_pyx.pyx";
    let hash = digest(&root.join(name))?;
    if hash != "a8cc5e5296dd38a012c55b0867c4563f8e320d25960260d897675dd90f0ea544" {
        return Err("changed original Params source".into());
    }
    source.insert(name.into(), hash);
    Ok(source)
}
pub fn run(repository: &Path, options: &Options) -> Result<J> {
    // An attempted run invalidates prior success even when preflight or signal
    // registration fails, without creating new output before source admission.
    match fs::remove_file(options.output.join("manifest.json")) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let pinned = sources(repository)?;
    let cancel = Cancellation::new()?;
    let result = run_inner(repository, options, &cancel, &pinned);
    if let Some(signal) = cancel.signal() {
        return Err(Interrupted(signal).into());
    }
    result
}
fn run_inner(
    repository: &Path,
    options: &Options,
    cancel: &Cancellation,
    pinned: &BTreeMap<String, String>,
) -> Result<J> {
    fs::create_dir_all(&options.output)?;
    let mut counts = Vec::new();
    let mut divergences = Vec::new();
    let mut comparisons = 0;
    for name in [
        "gates",
        "files",
        "scan",
        "reporting-loop",
        "disabled-loop",
        "text",
        "subprocess",
    ] {
        let mut pair = Pair::new(repository, options, &options.output.join(name), cancel)?;
        schedule::run(name, &mut pair)?;
        divergences.extend(
            pair.rows
                .iter()
                .filter(|v| {
                    v.get("passed")
                        .is_some_and(|v| matches!(v.view(), JsonView::Bool(false)))
                })
                .cloned(),
        );
        let count = pair.finish()?;
        comparisons += count;
        counts.push((name, J::parse(&count.to_string())?));
    }
    if &sources(repository)? != pinned {
        return Err("tombstone source changed during comparison".into());
    }
    let source_names = [
        "openpilot/system/tombstoned.py",
        "openpilot/system/sentry.py",
        "openpilot/system/athena/registration.py",
        "openpilot/common/params_pyx.pyx",
    ];
    let source_hashes = object(
        &source_names
            .iter()
            .map(|name| (*name, J::text(&pinned[*name])))
            .collect::<Vec<_>>(),
    )?;
    let mut command = options.runner.clone();
    command.push(options.binary.as_os_str().to_owned());
    let report = object(&[
        (
            "result",
            J::text(if divergences.is_empty() {
                "PASS"
            } else {
                "EMULATION_LIMITS"
            }),
        ),
        ("divergences", array(&divergences)?),
        (
            "matching_comparisons",
            J::parse(&(comparisons - divergences.len()).to_string())?,
        ),
        ("counts", object(&counts)?),
        ("comparisons", J::parse(&comparisons.to_string())?),
        ("binary_sha256", J::text(&digest(&options.binary)?)),
        (
            "params_binding_sha256",
            J::text(&digest(&options.params_binding)?),
        ),
        ("source_sha256", source_hashes),
        (
            "command",
            array(
                &command
                    .iter()
                    .map(|v| crate::python_fs::json_value(v))
                    .collect::<Vec<_>>(),
            )?,
        ),
        (
            "source_implementation",
            J::text("native independent source reference"),
        ),
        (
            "checker_sha256",
            J::text(&digest(&std::env::current_exe()?)?),
        ),
    ])?;
    fs::write(
        options.output.join("manifest.json"),
        format!("{}\n", format_report(&report)?),
    )?;
    Ok(report)
}

/// Cleanup only descendants carrying this invocation's private owner tag, after
/// the caller's direct worker/group owner has terminated and reaped its leader.
#[cfg(feature = "tombstone-live-native")]
pub(crate) fn cleanup_owned_helpers(token: &Path) -> Result<()> {
    files::cleanup_tagged(token)
}
