//! Actual tombstoned/collector/filesystem/HTTP QA, independent original policy.
use crate::{
    path_resolve::resolve_non_strict,
    seal::{digest, Result},
    version_reference_check::{process::Cancellation, Interrupted},
};
use openpilot_log_format::JsonValue as J;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
pub(crate) mod common;
mod worker;
use common::{array, number, object, pretty, require, Work};
#[derive(Clone)]
pub struct Options {
    pub daemon: PathBuf,
    pub collector: PathBuf,
    pub params_binding: PathBuf,
    pub output: PathBuf,
    pub environment: BTreeMap<OsString, OsString>,
}
pub fn parse(
    args: &[OsString],
    cwd: &Path,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<Options> {
    require(
        args.len() == 4,
        "tombstone runtime requires DAEMON COLLECTOR PARAMS_BINDING OUTPUT",
    )?;
    Ok(Options {
        daemon: resolve_non_strict(&cwd.join(&args[0]))?,
        collector: resolve_non_strict(&cwd.join(&args[1]))?,
        params_binding: resolve_non_strict(&cwd.join(&args[2]))?,
        output: resolve_non_strict(&cwd.join(&args[3]))?,
        environment: environment.clone(),
    })
}
pub fn format_report(value: &J) -> Result<String> {
    crate::control_tools_policy_check::json::pretty(value, 0)
}
pub(crate) fn sources(repository: &Path) -> Result<BTreeMap<String, String>> {
    let mut sources = crate::tombstone_reference_check::sources(repository)?;
    for &(name, expected) in LIVE_SOURCES {
        if RETIRED_CHECKERS.contains(&name) {
            match fs::symlink_metadata(repository.join(name)) {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            }
        }
        let actual = digest(&repository.join(name))?;
        require(
            actual == expected,
            format!("changed tombstone live input {name}"),
        )?;
        sources.insert(name.into(), actual);
    }
    Ok(sources)
}
pub fn source_provenance(repository: &Path) -> Result<J> {
    let live = sources(repository)?;
    let historical = LIVE_SOURCES
        .iter()
        .filter(|(name, _)| RETIRED_CHECKERS.contains(name) && !live.contains_key(*name))
        .copied()
        .collect::<BTreeMap<_, _>>();
    Ok(J::parse(
        &serde_json::json!({"source_sha256":live,"historical_source_sha256":historical})
            .to_string(),
    )?)
}
pub fn run(repository: &Path, options: &Options, worker: &Path) -> Result<J> {
    common::clear_manifest(&options.output)?;
    let pinned = sources(repository)?;
    let artifact_paths = [
        ("daemon", options.daemon.as_path()),
        ("collector", options.collector.as_path()),
        ("binding", options.params_binding.as_path()),
        ("worker", worker),
    ];
    let artifacts = common::artifact_hashes(&artifact_paths)?;
    let cancel = Cancellation::new()?;
    let result = (|| {
        fs::create_dir_all(&options.output)?;
        let mut cases = Vec::new();
        for (name, enabled) in [("enabled", true), ("pc-disabled", false)] {
            cases.push(common::work(
                Work {
                    repository,
                    output: &options.output.join(name),
                    daemon: &options.daemon,
                    collector: &options.collector,
                    binding: &options.params_binding,
                    worker,
                    runner: &[],
                    environment: &options.environment,
                    kind: "runtime",
                    enabled,
                    signal: 0,
                    timeout: Duration::from_secs(70),
                },
                &cancel,
            )?);
        }
        let failures = common::work(
            Work {
                repository,
                output: &options.output.join("startup"),
                daemon: &options.daemon,
                collector: &options.collector,
                binding: &options.params_binding,
                worker,
                runner: &[],
                environment: &options.environment,
                kind: "startup",
                enabled: false,
                signal: 0,
                timeout: Duration::from_secs(25),
            },
            &cancel,
        )?;
        fs::write(
            options.output.join("startup-failures.json"),
            format_report(&failures)? + "\n",
        )?;
        require(
            sources(repository)? == pinned,
            "tombstone live sources changed",
        )?;
        require(
            common::artifact_hashes(&artifact_paths)? == artifacts,
            "tombstone live artifact changed during checks",
        )?;
        pretty(
            &options.output.join("source-provenance.json"),
            &source_provenance(repository)?,
        )?;
        let report = object(&[
            ("result", J::text("PASS")),
            ("daemon_sha256", J::text(&artifacts["daemon"])),
            ("collector_sha256", J::text(&artifacts["collector"])),
            (
                "implementation",
                J::text(
                    "native Rust source policy, cereal/msgq and HTTP receiver; no Python execution",
                ),
            ),
            ("cases", array(&cases)?),
            ("startup_failure_cases", number(4)?),
            ("worker_sha256", J::text(&artifacts["worker"])),
            (
                "checker_sha256",
                J::text(&digest(&std::env::current_exe()?)?),
            ),
            ("params_binding_sha256", J::text(&artifacts["binding"])),
        ])?;
        cancel.check()?;
        pretty(&options.output.join("manifest.json"), &report)?;
        Ok(report)
    })();
    if let Some(signal) = cancel.signal() {
        common::clear_manifest(&options.output)?;
        return Err(Interrupted(signal).into());
    }
    result
}
pub fn worker_main() -> Result<()> {
    worker::main()
}
pub fn cli() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let options = parse(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &cwd,
        &std::env::vars_os().collect(),
    )?;
    let repository = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let worker = std::env::current_exe()?
        .parent()
        .ok_or("runtime executable directory absent")?
        .join("tombstone-runtime-worker");
    println!("{}", format_report(&run(&repository, &options, &worker)?)?);
    Ok(())
}

const LIVE_SOURCES: &[(&str, &str)] = &[
    (
        "rust/tools/check_tombstoned_runtime.py",
        "d167803d412098921a910578db3c45184df24cfd6a4fae88034edfffce2a203b",
    ),
    (
        "rust/tools/check_tombstoned_shutdown.py",
        "e77e4f6d70242cf0cbc82e0d70c35f54b63e8f265d7bcc4d66196ca8cd1d688c",
    ),
    (
        "rust/tools/tombstoned_reference.py",
        "7b647dba322d93e5d2bd9c5692356b10d3f85f2a2e0f9da8b982e05206c8e781",
    ),
    (
        "rust/tools/check_tombstoned_reference.py",
        "6271ece66356f377357ffc2b37bf3d98fcdb4c8231c8c07f556ed0294c261cd4",
    ),
    (
        "rust/tools/logmessaged_native.py",
        "3366e64eb36b0bad9b10023c46217f61c66aae0fe823120ae23d597d2456c872",
    ),
    (
        "rust/tools/logmessaged_reference.py",
        "ce6d6764911daaab1e9e5ce70152d9f5efc9b5ea006482064bb8192d36f735e3",
    ),
    (
        "rust/tools/check_crash_sdk_transport.py",
        "8bf1281bbc182ecaba9a29b9e49aa3d250ec8ee1f43eca5861596cee475ed806",
    ),
    (
        "openpilot/system/tombstoned.py",
        "a7a5f32535fe7c3a74593b3252366db5ac1c8b43f64837293e1c192f517527a5",
    ),
    (
        "openpilot/system/sentry.py",
        "fec900bf600e7a714460d8494472d6cc648759abf36ff72b79158bff61135fd8",
    ),
    (
        "openpilot/cereal/log.capnp",
        "fdecc98871c0ebc11417a558a600de6ff96cc51c2ce620122d39b2cee55f1620",
    ),
    (
        "openpilot/cereal/services.py",
        "a0745e2ef00036f0eef08b453a4798b4281549ea9ac6dc91c8b348ef76131edb",
    ),
];
const RETIRED_CHECKERS: &[&str] = &[
    "rust/tools/check_tombstoned_runtime.py",
    "rust/tools/check_tombstoned_shutdown.py",
];
