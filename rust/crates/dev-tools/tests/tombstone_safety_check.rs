//! Original harmless-marker proof exercises real source and native retrace paths.
//! Parent promotes this gate only when its Cargo registration is ready for RED.
#![cfg(feature = "tombstone-tools-native")]

use openpilot_dev_tools::tombstone_safety_check::{self as safety, Options};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}
fn environment() -> BTreeMap<OsString, OsString> {
    std::env::vars_os().collect()
}
fn artifact(name: &str) -> PathBuf {
    fs::canonicalize(std::env::var_os(name).unwrap_or_else(|| panic!("{name} required"))).unwrap()
}

#[test]
fn original_three_paths_and_raw_runner_args_are_preserved_without_source_mode() {
    use std::os::unix::ffi::OsStringExt;
    let root = tempfile::tempdir().unwrap();
    let raw = OsString::from_vec(b"runner-\xff".to_vec());
    let args = [
        "probe".into(),
        "binding".into(),
        "evidence".into(),
        "/bin/sh".into(),
        raw.clone(),
    ];
    let options = safety::parse(&args, root.path(), &environment()).unwrap();
    assert_eq!(options.binary, root.path().join("probe"));
    assert_eq!(options.params_binding, root.path().join("binding"));
    assert_eq!(options.output, root.path().join("evidence"));
    assert_eq!(options.runner, vec![OsString::from("/bin/sh"), raw]);
    for count in 0..3 {
        assert!(safety::parse(&args[..count], root.path(), &environment()).is_err());
    }
    assert!(safety::parse(&["--source".into()], root.path(), &environment()).is_err());
}

#[test]
fn four_actual_retraces_preserve_approved_markers_literal_input_and_normal_spaces() {
    let output = tempfile::tempdir().unwrap();
    let options = Options {
        binary: artifact("TOMBSTONE_PROBE_BINARY"),
        params_binding: artifact("TOMBSTONE_PARAMS_BINDING"),
        runner: vec![],
        output: output.path().join("safety"),
        environment: environment(),
    };
    let report = safety::run(&repository(), &options).unwrap();
    let value: Value = serde_json::from_str(&report.to_json().unwrap()).unwrap();
    assert_eq!(value["result"], "PASS");
    assert_eq!(value["source_issue"], 77);
    assert_eq!(value["ordinary_spaces_preserved"], true);
    assert_eq!(
        value["approved_difference"],
        "source executes shell substitutions; native passes filename as a positional argument"
    );
    let body = concat!(
        "ExecutablePath: /data/openpilot/selfdrive/modeld/modeld\n",
        "Signal: 11\nProcMaps:\n omitted\nProcStatus:\n kept\nCoreDump: omitted\n"
    );
    assert_eq!(
        value["cases"],
        Value::Array(
            [
                "x$(touch marker_substitution).crash",
                "x`touch marker_backtick`.crash",
                "x\"$(touch marker_quote)\".crash",
            ]
            .into_iter()
            .map(|filename| json!({
                "filename": filename,
                "source_marker_created": true,
                "native_marker_created": false,
                "source_retrace_input": "Package: openpilot\n",
                "native_retrace_input": format!("Package: openpilot\n{body}")
            }))
            .collect()
        )
    );
    assert_eq!(
        value["binary_sha256"],
        format!("{:x}", Sha256::digest(fs::read(&options.binary).unwrap()))
    );
    assert_eq!(
        fs::read_to_string(options.output.join("safety.json")).unwrap(),
        format!("{}\n", safety::format_report(&report).unwrap())
    );
    let comparisons = fs::read_to_string(options.output.join("comparisons.jsonl")).unwrap();
    let rows = comparisons
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 5, "configure plus four original retraces");
    assert!(rows.iter().all(|row| row["passed"] == true));
    assert_eq!(
        rows[4]["request"]["path"],
        "$FIXTURE/apport/ordinary space.crash"
    );
}

#[test]
fn changed_source_preflight_cannot_leave_a_stale_success_report() {
    let repository = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let stale = output.path().join("safety.json");
    fs::write(&stale, b"{\"result\":\"PASS\"}\n").unwrap();
    let options = Options {
        binary: PathBuf::from("/absent-probe"),
        params_binding: PathBuf::from("/absent-binding"),
        output: output.path().to_owned(),
        runner: vec![],
        environment: environment(),
    };
    assert!(safety::run(repository.path(), &options).is_err());
    assert!(
        !stale.exists(),
        "failed attempt retained an old safety success"
    );
}

#[test]
fn actual_cli_rejects_missing_arguments_and_source_mode_without_success_output() {
    for args in [Vec::<OsString>::new(), vec!["--source".into()]] {
        let result = Command::new(env!("CARGO_BIN_EXE_check-tombstoned-safety"))
            .args(args)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert!(!result.stderr.is_empty());
    }
}

#[test]
fn only_retired_safety_checker_may_be_absent_changed_and_dangling_files_still_reject() {
    use std::os::unix::fs::symlink;
    let original = repository();
    let copied = tempfile::tempdir().unwrap();
    for name in [
        "openpilot/system/tombstoned.py",
        "openpilot/system/sentry.py",
        "openpilot/system/athena/registration.py",
        "openpilot/system/version.py",
        "openpilot/common/git.py",
        "openpilot/common/utils.py",
        "openpilot/common/params_pyx.pyx",
        "rust/tools/check_version_reference.py",
        "rust/tools/tombstoned_reference.py",
        "rust/tools/check_tombstoned_reference.py",
        "rust/crates/dev-tools/src/tombstone_reference_check/apport-retrace",
    ] {
        let target = copied.path().join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        if name.starts_with("rust/tools/") && !original.join(name).try_exists().unwrap() {
            continue;
        }
        symlink(original.join(name), target).unwrap();
    }
    let options = Options {
        binary: artifact("TOMBSTONE_PROBE_BINARY"),
        params_binding: artifact("TOMBSTONE_PARAMS_BINDING"),
        runner: vec![],
        output: copied.path().join("proof"),
        environment: environment(),
    };
    let retired = copied.path().join("rust/tools/check_tombstoned_safety.py");
    assert!(!retired.try_exists().unwrap());
    let report = safety::run(copied.path(), &options).unwrap();
    assert!(report.get("result").unwrap().text_eq("PASS"));
    let provenance: Value =
        serde_json::from_slice(&fs::read(options.output.join("source-provenance.json")).unwrap())
            .unwrap();
    assert!(provenance["source_sha256"]
        .get("rust/tools/check_tombstoned_safety.py")
        .is_none());
    assert_eq!(
        provenance["historical_source_sha256"]["rust/tools/check_tombstoned_safety.py"],
        "2b70f9cd79cde571fed12d5f15f06ff080786de23de54cc47dd0187d9862f0a4"
    );
    fs::write(&retired, b"changed reintroduced checker\n").unwrap();
    assert!(safety::run(copied.path(), &options).is_err());
    assert!(!options.output.join("safety.json").exists());
    fs::remove_file(&retired).unwrap();
    symlink(copied.path().join("missing-original"), &retired).unwrap();
    assert!(safety::run(copied.path(), &options).is_err());
    assert!(!options.output.join("safety.json").exists());
    fs::remove_file(&retired).unwrap();
    fs::remove_file(copied.path().join("openpilot/system/tombstoned.py")).unwrap();
    assert!(
        safety::run(copied.path(), &options).is_err(),
        "shared live Worker source was silently retired with standalone safety checker"
    );
}
