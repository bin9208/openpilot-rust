//! Actual held-apport signal comparison, retaining strict native cleanup limits.
use crate::{
    argparse_scan::{scan_os_with_positionals, OptionSpec},
    path_resolve::resolve_non_strict,
    seal::Result,
    tombstone_runtime_check::{
        common::{self, array, field, object, pretty, require, Work},
        sources,
    },
    version_reference_check::{process::Cancellation, Interrupted},
};
use openpilot_log_format::{JsonValue as J, JsonView};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Clone)]
pub struct Options {
    pub binary: PathBuf,
    pub params_binding: PathBuf,
    pub output: PathBuf,
    pub observe: bool,
    pub runner: Vec<OsString>,
    pub environment: BTreeMap<OsString, OsString>,
}
pub enum Command {
    Help,
    Run(Options),
}
pub fn parse(
    args: &[OsString],
    cwd: &Path,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<Command> {
    // argparse.REMAINDER consumes even option-looking arguments after --runner.
    let mut enabled = true;
    let mut split = None;
    for (index, arg) in args.iter().enumerate() {
        if arg == "--" {
            enabled = false;
            continue;
        }
        if enabled
            && arg
                .to_str()
                .is_some_and(|v| v.len() > 2 && v.starts_with("--") && "--runner".starts_with(v))
        {
            split = Some(index);
            break;
        }
    }
    let (head, runner) = split.map_or((args, &[][..]), |index| {
        (&args[..index], &args[index + 1..])
    });
    let parsed = scan_os_with_positionals(head, &[OptionSpec::flag("--observe", None)], 3)?;
    if parsed.help {
        return Ok(Command::Help);
    }
    Ok(Command::Run(Options {
        binary: resolve_non_strict(&cwd.join(&parsed.positionals[0]))?,
        params_binding: resolve_non_strict(&cwd.join(&parsed.positionals[1]))?,
        output: resolve_non_strict(&cwd.join(&parsed.positionals[2]))?,
        observe: parsed.values.contains_key("--observe"),
        runner: runner.to_vec(),
        environment: environment.clone(),
    }))
}
pub fn format_report(report: &J) -> Result<String> {
    crate::control_tools_policy_check::json::pretty(report, 0)
}
pub fn run(repository: &Path, options: &Options, worker: &Path) -> Result<J> {
    common::clear_manifest(&options.output)?;
    let pinned = sources(repository)?;
    let artifact_paths = [
        ("daemon", options.binary.as_path()),
        ("binding", options.params_binding.as_path()),
        ("worker", worker),
    ];
    let artifacts = common::artifact_hashes(&artifact_paths)?;
    let cancel = Cancellation::new()?;
    let result = (|| {
        fs::create_dir_all(&options.output)?;
        let mut cases = Vec::new();
        for (kind, signal, name) in [
            ("shutdown-source", 2, "source-SIGINT"),
            ("shutdown-native", 2, "native-SIGINT"),
            ("shutdown-native", 15, "native-SIGTERM"),
        ] {
            cases.push(common::work(
                Work {
                    repository,
                    output: &options.output.join(name),
                    daemon: &options.binary,
                    collector: Path::new(""),
                    binding: &options.params_binding,
                    worker,
                    runner: &options.runner,
                    environment: &options.environment,
                    kind,
                    enabled: true,
                    signal,
                    timeout: Duration::from_secs(40),
                },
                &cancel,
            )?);
        }
        let truth = |row: &J, key: &str| {
            row.get(key)
                .is_some_and(|v| matches!(v.view(), JsonView::Bool(true)))
        };
        let native_pass = cases
            .iter()
            .filter(|v| v.get("kind").is_some_and(|v| v.text_eq("native")))
            .all(|row| {
                truth(row, "exited_within_5_seconds")
                    && row
                        .get("exit_code")
                        .is_some_and(|v| v.to_json().is_ok_and(|v| v == "0"))
                    && row
                        .get("active_descendants_after_stop")
                        .is_some_and(|v| matches!(v.view(),JsonView::Array(v)if v.is_empty()))
                    && truth(row, "crash_retained")
                    && !truth(row, "crash_copied")
                    && row
                        .get("http_events")
                        .is_some_and(|v| v.to_json().is_ok_and(|v| v == "0"))
            });
        require(
            truth(&cases[0], "exited_within_5_seconds")
                && field(&cases[0], "exit_code")?.to_json()? == "-2",
            "independent source SIGINT status/budget differs",
        )?;
        require(
            sources(repository)? == pinned,
            "shutdown sources changed during checks",
        )?;
        require(
            common::artifact_hashes(&artifact_paths)? == artifacts,
            "tombstone shutdown artifact changed during checks",
        )?;
        pretty(
            &options.output.join("source-provenance.json"),
            &crate::tombstone_runtime_check::source_provenance(repository)?,
        )?;
        let report=object(&[("result",J::text(if native_pass{"PASS"}else{"FAIL"})),("binary_sha256",J::text(&artifacts["daemon"])),("source_sha256",J::text(pinned.get("openpilot/system/tombstoned.py").ok_or("missing source policy pin")?)),("cases",array(&cases)?),("implementation",J::text("native independent source loop; actual native daemon, HTTP and pidfd-owned helpers")),("worker_sha256",J::text(&artifacts["worker"])),("params_binding_sha256",J::text(&artifacts["binding"]))])?;
        cancel.check()?;
        pretty(&options.output.join("manifest.json"), &report)?;
        if !options.observe {
            require(
                native_pass,
                "native stop budget or owned child cleanup failed",
            )?;
        }
        Ok(report)
    })();
    if let Some(signal) = cancel.signal() {
        common::clear_manifest(&options.output)?;
        return Err(Interrupted(signal).into());
    }
    result
}
pub fn cli() -> Result<()> {
    let cwd = std::env::current_dir()?;
    match parse(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &cwd,
        &std::env::vars_os().collect(),
    )? {
        Command::Help => {
            println!("usage: check-tombstoned-shutdown [-h] [--observe] [--runner ...] binary binding output");
            Ok(())
        }
        Command::Run(options) => {
            let repository = std::env::var_os("OPENPILOT_RUST_ROOT")
                .map(PathBuf::from)
                .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
            let worker = std::env::current_exe()?
                .parent()
                .ok_or("shutdown checker executable directory missing")?
                .join("tombstone-runtime-worker");
            println!("{}", format_report(&run(&repository, &options, &worker)?)?);
            Ok(())
        }
    }
}
