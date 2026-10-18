//! Shared owned worker boundaries for the two original live checker schedules.
use crate::{
    qa_endpoint::EndpointOwner,
    qa_process::{CaptureError, CommandSpec, NativeRunner},
    seal::{digest, Result},
    version_reference_check::process::Cancellation,
};
use openpilot_log_format::JsonValue as J;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
pub(crate) fn object(fields: &[(&str, J)]) -> Result<J> {
    Ok(J::parse(&format!(
        "{{{}}}",
        fields
            .iter()
            .map(|(key, value)| Ok(format!("{}:{}", J::text(key).to_json()?, value.to_json()?)))
            .collect::<Result<Vec<_>>>()?
            .join(",")
    ))?)
}
pub(crate) fn array(values: &[J]) -> Result<J> {
    Ok(J::parse(&format!(
        "[{}]",
        values
            .iter()
            .map(J::to_json)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join(",")
    ))?)
}
pub(crate) fn number(value: impl ToString) -> Result<J> {
    Ok(J::parse(&value.to_string())?)
}
pub(crate) fn boolean(value: bool) -> J {
    J::parse(if value { "true" } else { "false" }).expect("boolean JSON")
}
pub(crate) fn field(value: &J, key: &str) -> Result<J> {
    value
        .get(key)
        .ok_or_else(|| format!("missing live tombstone field {key}").into())
}
pub(crate) fn require(value: bool, error: impl Into<String>) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(error.into().into())
    }
}
pub(crate) fn pretty(path: &Path, value: &J) -> Result<()> {
    fs::write(
        path,
        crate::control_tools_policy_check::json::pretty(value, 0)? + "\n",
    )?;
    Ok(())
}
pub(crate) fn path(value: &[u8]) -> PathBuf {
    OsString::from_vec(value.to_vec()).into()
}
pub(crate) fn clear_manifest(output: &Path) -> Result<()> {
    match fs::remove_file(output.join("manifest.json")) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
pub(crate) fn artifact_hashes(paths: &[(&str, &Path)]) -> Result<BTreeMap<String, String>> {
    paths
        .iter()
        .map(|(name, path)| Ok(((*name).to_owned(), digest(path)?)))
        .collect()
}
#[derive(Serialize, Deserialize)]
pub(crate) struct Request {
    pub repository: Vec<u8>,
    pub root: Vec<u8>,
    pub output: Vec<u8>,
    pub daemon: Vec<u8>,
    pub collector: Vec<u8>,
    pub binding: Vec<u8>,
    pub worker: Vec<u8>,
    pub runner: Vec<Vec<u8>>,
    pub prefix: String,
    pub nonce: String,
    pub endpoint_channel: Option<Vec<u8>>,
    pub kind: String,
    pub enabled: bool,
    pub signal: i32,
    pub worker_sha256: String,
    pub sources: BTreeMap<String, String>,
}
struct Namespace {
    _directory: tempfile::TempDir,
    prefix: String,
    endpoint: PathBuf,
}
impl Namespace {
    fn new() -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("msgq_tombstone-live-")
            .tempdir_in("/dev/shm")?;
        let prefix = directory
            .path()
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_prefix("msgq_"))
            .ok_or("private tombstone namespace invalid")?
            .to_owned();
        let endpoint = PathBuf::from(format!("/tmp/logmessage{prefix}"));
        match fs::symlink_metadata(&endpoint) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => return Err("tombstone endpoint existed before launch".into()),
        }
        Ok(Self {
            _directory: directory,
            prefix,
            endpoint,
        })
    }
}
pub(crate) struct Work<'a> {
    pub repository: &'a Path,
    pub output: &'a Path,
    pub daemon: &'a Path,
    pub collector: &'a Path,
    pub binding: &'a Path,
    pub worker: &'a Path,
    pub runner: &'a [OsString],
    pub environment: &'a BTreeMap<OsString, OsString>,
    pub kind: &'a str,
    pub enabled: bool,
    pub signal: i32,
    pub timeout: Duration,
}
/// Root, nonce, IPC namespace and descendant token exist before launch. Cleanup
/// never trusts a persistent/reusable output receipt to claim an IPC path.
pub(crate) fn work(options: Work<'_>, cancel: &Cancellation) -> Result<J> {
    fs::create_dir_all(options.output.parent().ok_or("live output parent absent")?)?;
    fs::create_dir(options.output)?;
    let fixture = tempfile::Builder::new()
        .prefix("tombstone-live-")
        .tempdir()?;
    let request_home = tempfile::Builder::new()
        .prefix("tombstone-live-request-")
        .tempdir()?;
    let namespace = Namespace::new()?;
    let nonce = request_home
        .path()
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("invalid live nonce")?
        .to_owned();
    // Only runtime starts a collector. Startup and shutdown never claim an
    // endpoint or wait for a readiness message that their workers do not send.
    let endpoint = (options.kind == "runtime")
        .then(|| EndpointOwner::new(&namespace.endpoint, &nonce))
        .transpose()?;
    let request = Request {
        repository: options.repository.as_os_str().as_bytes().to_vec(),
        root: fixture.path().as_os_str().as_bytes().to_vec(),
        output: options.output.as_os_str().as_bytes().to_vec(),
        daemon: options.daemon.as_os_str().as_bytes().to_vec(),
        collector: options.collector.as_os_str().as_bytes().to_vec(),
        binding: options.binding.as_os_str().as_bytes().to_vec(),
        worker: options.worker.as_os_str().as_bytes().to_vec(),
        runner: options
            .runner
            .iter()
            .map(|v| v.as_bytes().to_vec())
            .collect(),
        prefix: namespace.prefix.clone(),
        nonce: nonce.clone(),
        endpoint_channel: endpoint
            .as_ref()
            .map(|owner| owner.channel().as_os_str().as_bytes().to_vec()),
        kind: options.kind.into(),
        enabled: options.enabled,
        signal: options.signal,
        worker_sha256: digest(options.worker)?,
        sources: super::sources(options.repository)?,
    };
    let request_path = request_home.path().join("request.json");
    fs::write(&request_path, serde_json::to_vec(&request)?)?;
    let mut command = CommandSpec::new_os(
        vec![
            options.worker.as_os_str().to_owned(),
            "--request".into(),
            request_path.into_os_string(),
        ],
        options.timeout,
    );
    let mut environment = options.environment.clone();
    environment.insert("OPENPILOT_PREFIX".into(), namespace.prefix.clone().into());
    environment.insert(
        "TOMBSTONE_REFERENCE_OWNER".into(),
        fixture.path().as_os_str().to_owned(),
    );
    command.env_remove = std::env::vars_os()
        .map(|(key, _)| key)
        .filter(|key| !environment.contains_key(key))
        .collect();
    command.set_os_env(environment);
    command.inherit_stdin = true;
    command.stdout_limit = usize::MAX;
    command.stderr_limit = usize::MAX;
    // NativeRunner finishes root group cleanup before the token-based cleanup of
    // apport's deliberately separate child groups and before any IPC unlinks.
    let captured =
        NativeRunner.run_controlled(&command, Instant::now() + command.timeout, &cancel.flag());
    let cleanup = crate::tombstone_reference_check::cleanup_owned_helpers(fixture.path());
    let captured = match captured {
        Ok(value) => value,
        Err(error) => {
            if let Some(CaptureError::Timeout { stdout, stderr, .. }) =
                error.downcast_ref::<CaptureError>()
            {
                fs::write(options.output.join("worker.stdout"), stdout)?;
                fs::write(options.output.join("worker.stderr"), stderr)?;
            }
            cleanup?;
            return Err(error);
        }
    };
    cleanup?;
    fs::write(options.output.join("worker.stdout"), &captured.stdout)?;
    fs::write(options.output.join("worker.stderr"), &captured.stderr)?;
    require(
        captured.code == 0,
        format!(
            "tombstone live worker exited {}: {}",
            captured.code,
            String::from_utf8_lossy(&captured.stderr)
        ),
    )?;
    if let Some(endpoint) = &endpoint {
        endpoint.verify()?;
    }
    let reply = J::parse(std::str::from_utf8(&captured.stdout)?)?;
    require(
        field(&reply, "nonce")?.text_eq(&nonce),
        "tombstone worker reply nonce mismatch",
    )?;
    require(
        super::sources(options.repository)? == request.sources,
        "live source changed while worker ran",
    )?;
    cancel.check()?;
    field(&reply, "report")
}
