//! Outer process owner: every comparison gets a new worker, filesystem and IPC
//! namespace. No Msgq context or environment mutation occurs in this library.
use super::{cases, compare, require, Options};
use crate::{
    qa_process::{CaptureError, CommandSpec, NativeRunner},
    seal::{digest, Result},
    version_reference_check::{process::Cancellation, Interrupted},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::PermissionsExt,
    },
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Serialize, Deserialize)]
pub(super) struct Request {
    pub repository: Vec<u8>,
    pub binary: Vec<u8>,
    pub collector: Vec<u8>,
    pub binding: Vec<u8>,
    pub worker: Vec<u8>,
    pub root: Vec<u8>,
    pub output: Vec<u8>,
    pub runner: Vec<Vec<u8>>,
    pub prefix: String,
    pub nonce: String,
    pub endpoint_channel: Vec<u8>,
    pub source: bool,
    pub original_collector: bool,
    pub gated: bool,
    pub scenario: Value,
    pub artifacts: BTreeMap<String, String>,
    pub sources: BTreeMap<String, String>,
}
pub(super) fn path(raw: &[u8]) -> PathBuf {
    OsString::from_vec(raw.to_vec()).into()
}
pub(super) fn pretty(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(value)? + "\n")?;
    Ok(())
}
const RETIRED_SOURCES: &[(&str, &str)] = &[
    (
        "rust/tools/check_managed_entry.py",
        "89d2ce945fe94955d9b764f6a71434ac02298713a8983513bb9990318e6acc44",
    ),
    (
        "rust/tools/check_managed_entry_readiness.py",
        "d353d34ce4a6d633710c981d66bd571700f919c9ffb8800c6fd1273acabe732e",
    ),
    (
        "rust/tools/managed_entry_reference.py",
        "3f502d9c478e5f801f84b52ab5b613cb55f5d161b68434568e3942a72ac642d8",
    ),
];
pub(super) fn sources(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut pins = crate::tombstone_source::sources(root)?;
    pins.extend(crate::managed_entry_collector_source::sources(root)?);
    for (name, expected) in [
        (
            "msgq_repo/msgq/msgq.h",
            "ee16b5b2cccd5fceb526f2f3e85a3ffe0fd523b25fdb0a9cde72fb03ee0183bc",
        ),
        (
            "rust/tools/logmessaged_reference.py",
            "ce6d6764911daaab1e9e5ce70152d9f5efc9b5ea006482064bb8192d36f735e3",
        ),
        (
            "rust/tools/logmessaged_native.py",
            "3366e64eb36b0bad9b10023c46217f61c66aae0fe823120ae23d597d2456c872",
        ),
        (
            "rust/tools/logging_producer_reference.py",
            "ac89bd78f4a2eed348317e374c4f21a9d2e28500abbeab10b50313ef3677a536",
        ),
        (
            "openpilot/system/manager/process.py",
            "c824eded96e88533f4d150a1b2ea0262cfd076cffea0692fad52ef4ba3bf5c8e",
        ),
        (
            "openpilot/system/logmessaged.py",
            "93d94801185350ef4470bd482d37ed7f6d466e9b100ad10aa16fa6e8570fb02a",
        ),
        (
            "openpilot/common/logging_extra.py",
            "ce9b3b3d8498d76e1c4c0352a3e98e72759bb7f367ff8629c1329c46040595c3",
        ),
        (
            "openpilot/common/swaglog.py",
            "d37b623bd9418b72ffd3c4800a1da192c49856cb63bbbc1c09bb24a5ec6f8616",
        ),
        (
            "openpilot/common/params_pyx.pyx",
            "a8cc5e5296dd38a012c55b0867c4563f8e320d25960260d897675dd90f0ea544",
        ),
    ]
    .into_iter()
    .chain(RETIRED_SOURCES.iter().copied())
    {
        if RETIRED_SOURCES.iter().any(|(retired, _)| *retired == name) {
            match fs::symlink_metadata(root.join(name)) {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            }
        }
        let hash = digest(&root.join(name))?;
        require(
            hash == expected,
            &format!("changed managed-entry source {name}"),
        )?;
        pins.insert(name.into(), hash);
    }
    Ok(pins)
}
fn historical_sources(live: &BTreeMap<String, String>) -> BTreeMap<&'static str, &'static str> {
    RETIRED_SOURCES
        .iter()
        .filter(|(name, _)| !live.contains_key(*name))
        .copied()
        .collect()
}
pub(super) fn artifacts(
    binary: &Path,
    collector: &Path,
    binding: &Path,
    worker: &Path,
) -> Result<BTreeMap<String, String>> {
    [
        ("binary", binary),
        ("collector", collector),
        ("binding", binding),
        ("worker", worker),
    ]
    .into_iter()
    .map(|(name, path)| Ok((name.into(), digest(path)?)))
    .collect()
}
struct Namespace {
    owner: crate::qa_endpoint::EndpointOwner,
    _directory: tempfile::TempDir,
    prefix: String,
}
impl Namespace {
    fn new(nonce: &str) -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("msgq_managed-entry-")
            .tempdir_in("/dev/shm")?;
        let prefix = directory
            .path()
            .file_name()
            .and_then(|value| value.to_str())
            .and_then(|value| value.strip_prefix("msgq_"))
            .ok_or("invalid managed namespace")?
            .to_owned();
        let endpoint = PathBuf::from(format!("/tmp/logmessage{prefix}"));
        let owner = crate::qa_endpoint::EndpointOwner::new(&endpoint, nonce)?;
        Ok(Self {
            owner,
            _directory: directory,
            prefix,
        })
    }
}
struct Work<'a> {
    repository: &'a Path,
    options: &'a Options,
    output: &'a Path,
    worker: &'a Path,
    original_collector: bool,
    gated: bool,
    source: bool,
    scenario: &'a Value,
}
struct FixtureRoot(tempfile::TempDir);
impl Drop for FixtureRoot {
    fn drop(&mut self) {
        // The put-failure fixture deliberately removes write permission. This
        // parent-owned cleanup also runs when the worker is forcibly stopped.
        let _ = fs::set_permissions(
            self.0.path().join("params"),
            fs::Permissions::from_mode(0o700),
        );
    }
}
fn work(work: Work<'_>, cancel: &Cancellation) -> Result<Value> {
    fs::create_dir_all(work.output.parent().ok_or("managed output parent absent")?)?;
    fs::create_dir(work.output)?;
    let root = FixtureRoot(
        tempfile::Builder::new()
            .prefix("managed-entry-")
            .tempdir()?,
    );
    let request_home = tempfile::Builder::new()
        .prefix("managed-entry-request-")
        .tempdir()?;
    let nonce = request_home
        .path()
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("invalid managed nonce")?
        .to_owned();
    let namespace = Namespace::new(&nonce)?;
    let request = Request {
        repository: work.repository.as_os_str().as_bytes().to_vec(),
        binary: work.options.binary.as_os_str().as_bytes().to_vec(),
        collector: work.options.collector.as_os_str().as_bytes().to_vec(),
        binding: work.options.params_binding.as_os_str().as_bytes().to_vec(),
        worker: work.worker.as_os_str().as_bytes().to_vec(),
        root: root.0.path().as_os_str().as_bytes().to_vec(),
        output: work.output.as_os_str().as_bytes().to_vec(),
        runner: work
            .options
            .runner
            .iter()
            .map(|v| v.as_bytes().to_vec())
            .collect(),
        prefix: namespace.prefix.clone(),
        nonce: nonce.clone(),
        endpoint_channel: namespace.owner.channel().as_os_str().as_bytes().to_vec(),
        source: work.source,
        original_collector: work.original_collector,
        gated: work.gated,
        scenario: work.scenario.clone(),
        artifacts: artifacts(
            &work.options.binary,
            &work.options.collector,
            &work.options.params_binding,
            work.worker,
        )?,
        sources: sources(work.repository)?,
    };
    let request_path = request_home.path().join("request.json");
    pretty(&request_path, &serde_json::to_value(&request)?)?;
    let mut command = CommandSpec::new_os(
        vec![
            work.worker.as_os_str().to_owned(),
            "--managed-worker".into(),
            request_path.into_os_string(),
        ],
        Duration::from_secs(90),
    );
    let mut environment = work.options.environment.clone();
    environment.insert("OPENPILOT_PREFIX".into(), namespace.prefix.clone().into());
    environment.insert("RUST_BACKTRACE".into(), "1".into());
    command.env_remove = std::env::vars_os()
        .map(|(key, _)| key)
        .filter(|key| !environment.contains_key(key))
        .collect();
    command.set_os_env(environment);
    command.stdout_limit = usize::MAX;
    command.stderr_limit = usize::MAX;
    let captured =
        NativeRunner.run_controlled(&command, Instant::now() + command.timeout, &cancel.flag());
    let captured = match captured {
        Ok(value) => value,
        Err(error) => {
            if let Some(CaptureError::Timeout { stdout, stderr, .. }) =
                error.downcast_ref::<CaptureError>()
            {
                fs::write(work.output.join("worker.stdout"), stdout)?;
                fs::write(work.output.join("worker.stderr"), stderr)?;
            }
            return Err(error);
        }
    };
    fs::write(work.output.join("worker.stdout"), &captured.stdout)?;
    fs::write(work.output.join("worker.stderr"), &captured.stderr)?;
    require(
        captured.code == 0,
        &format!(
            "managed-entry worker exited {}: {}",
            captured.code,
            String::from_utf8_lossy(&captured.stderr)
        ),
    )?;
    namespace.owner.verify()?;
    let reply: Value = serde_json::from_slice(&captured.stdout)?;
    require(
        reply["nonce"] == nonce,
        "managed-entry worker nonce mismatch",
    )?;
    require(
        sources(work.repository)? == request.sources,
        "managed-entry source changed during worker",
    )?;
    require(
        artifacts(
            &work.options.binary,
            &work.options.collector,
            &work.options.params_binding,
            work.worker,
        )? == request.artifacts,
        "managed-entry artifact changed during worker",
    )?;
    cancel.check()?;
    super::field(&reply, "report").cloned()
}
pub fn run(
    repository: &Path,
    options: &Options,
    worker: &Path,
) -> Result<openpilot_log_format::JsonValue> {
    match fs::remove_file(options.output.join("manifest.json")) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let roster = cases()
        .into_iter()
        .filter(|(name, _)| {
            options
                .case
                .as_ref()
                .is_none_or(|selected| selected == name)
        })
        .collect::<Vec<_>>();
    require(
        !roster.is_empty(),
        "unknown managed-entry case would compare zero scenarios",
    )?;
    let pinned = sources(repository)?;
    let binaries = artifacts(
        &options.binary,
        &options.collector,
        &options.params_binding,
        worker,
    )?;
    let cancel = Cancellation::new()?;
    let result = (|| {
        fs::create_dir_all(&options.output)?;
        let mut results = Vec::new();
        for original_collector in [true, false] {
            let collector = if original_collector {
                "original"
            } else {
                "native"
            };
            for (name, scenario) in &roster {
                let target = options
                    .output
                    .join(format!("{collector}-collector"))
                    .join(name);
                let source = work(
                    Work {
                        repository,
                        options,
                        output: &target.join("source"),
                        worker,
                        original_collector,
                        gated: false,
                        source: true,
                        scenario,
                    },
                    &cancel,
                )?;
                let native = work(
                    Work {
                        repository,
                        options,
                        output: &target.join("native"),
                        worker,
                        original_collector,
                        gated: false,
                        source: false,
                        scenario,
                    },
                    &cancel,
                )?;
                let mut comparison = compare(&source, &native, scenario)?;
                comparison["collector"] = json!(collector);
                comparison["case"] = json!(name);
                pretty(&target.join("comparison.json"), &comparison)?;
                results.push(comparison);
            }
        }
        require(
            results.len() == roster.len() * 2,
            "incomplete managed-entry comparison roster",
        )?;
        require(
            sources(repository)? == pinned,
            "managed-entry final source changed",
        )?;
        require(
            artifacts(
                &options.binary,
                &options.collector,
                &options.params_binding,
                worker,
            )? == binaries,
            "managed-entry final artifact changed",
        )?;
        cancel.check()?;
        let historical = historical_sources(&pinned);
        let report = json!({"result":"PASS","comparisons":results.len(),"results":results,"source_sha256":pinned,"historical_source_sha256":historical,"artifact_sha256":binaries});
        let mut encoded = report.to_string();
        encoded.pop();
        encoded.push_str(",\"runner\":[");
        encoded.push_str(
            &options
                .runner
                .iter()
                .map(|arg| crate::python_fs::json_value(arg).to_json())
                .collect::<std::result::Result<Vec<_>, _>>()?
                .join(","),
        );
        encoded.push_str("]}");
        let report = openpilot_log_format::JsonValue::parse(&encoded)?;
        fs::write(
            options.output.join("manifest.json"),
            crate::control_tools_policy_check::json::pretty(&report, 0)? + "\n",
        )?;
        Ok(report)
    })();
    if let Some(signal) = cancel.signal() {
        let _ = fs::remove_file(options.output.join("manifest.json"));
        return Err(Interrupted(signal).into());
    }
    result
}

/// Exact original readiness schedule: source reset-error with the original
/// collector policy, forcing its error publisher to initialize after readers.
pub fn run_readiness(
    repository: &Path,
    options: &Options,
    worker: &Path,
) -> Result<openpilot_log_format::JsonValue> {
    match fs::remove_file(options.output.join("manifest.json")) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let pinned = sources(repository)?;
    let binaries = artifacts(
        &options.binary,
        &options.collector,
        &options.params_binding,
        worker,
    )?;
    let cancel = Cancellation::new()?;
    let result = (|| {
        fs::create_dir_all(&options.output)?;
        let scenario = json!({"body":"return","reset":"error"});
        let report = work(
            Work {
                repository,
                options,
                output: &options.output.join("reset-error"),
                worker,
                original_collector: true,
                gated: true,
                source: true,
                scenario: &scenario,
            },
            &cancel,
        )?;
        require(
            report["response"]["outcome"]["kind"] == "raised",
            "readiness source did not raise",
        )?;
        let errors = super::array(super::field(&report, "records")?, "errorLogMessage")?;
        require(
            errors.len() == 1
                && report["records"]["errorLogMessage"] == report["records"]["logMessage"],
            "readiness crash publications differ",
        )?;
        require(
            sources(repository)? == pinned,
            "readiness source changed during check",
        )?;
        require(
            artifacts(
                &options.binary,
                &options.collector,
                &options.params_binding,
                worker,
            )? == binaries,
            "readiness artifact changed during check",
        )?;
        cancel.check()?;
        let historical = historical_sources(&pinned);
        let report = json!({"result":"PASS","scenario":"reset-error","schedule":"subscriber-before-publisher","error_messages":errors.len(),"source_sha256":pinned,"historical_source_sha256":historical,"artifact_sha256":binaries});
        pretty(&options.output.join("manifest.json"), &report)?;
        Ok(openpilot_log_format::JsonValue::parse(&report.to_string())?)
    })();
    if let Some(signal) = cancel.signal() {
        let _ = fs::remove_file(options.output.join("manifest.json"));
        return Err(Interrupted(signal).into());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::Namespace;
    use std::{fs, os::unix::net::UnixListener};

    #[test]
    fn observed_endpoint_identity_cannot_expand_to_a_replacement_socket() {
        let namespace = Namespace::new("unit-owned-endpoint").unwrap();
        let endpoint = std::path::PathBuf::from(format!("/tmp/logmessage{}", namespace.prefix));
        let original = UnixListener::bind(&endpoint).unwrap();
        crate::qa_endpoint::acknowledge(
            &endpoint,
            namespace.owner.channel(),
            "unit-owned-endpoint",
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        namespace.owner.verify().unwrap();
        fs::remove_file(&endpoint).unwrap();
        let replacement = UnixListener::bind(&endpoint).unwrap();
        drop(namespace);
        let retained = fs::symlink_metadata(&endpoint).is_ok();
        // This test owns the replacement regardless of the assertion outcome.
        let _ = fs::remove_file(&endpoint);
        drop(replacement);
        drop(original);
        assert!(
            retained,
            "namespace cleanup removed a replacement foreign endpoint"
        );
    }
}
