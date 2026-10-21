//! Mandatory fresh five-case comparison to the retained Python updater oracle.
#![cfg(target_os = "linux")]
use openpilot_dev_tools::{
    qa_process::{CommandSpec, NativeRunner, Runner},
    updated_check::{assert_equivalent, cases, parse, run},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    env,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    time::Duration,
};

fn required(name: &str) -> PathBuf {
    PathBuf::from(
        env::var_os(name).unwrap_or_else(|| panic!("{name} is mandatory for updated comparison")),
    )
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn fresh_original_and_native_all_five_cases() {
    let output = required("UPDATED_CHECK_OUTPUT");
    let report = run(
        &required("UPDATED_CHECK_BINARY"),
        &required("UPDATED_CHECK_LAUNCHER"),
        &required("UPDATED_CHECK_BINDING"),
        &output,
        &required("UPDATED_CHECK_PYTHON"),
        &required("UPDATED_CHECK_SHIM"),
    )
    .unwrap();
    assert_eq!(report["passed"], true);
    assert_eq!(report["fresh_original_python_executed"], true);
    let names = cases().into_iter().map(|case| case.0).collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "check-fetch-metered",
            "missing-branch-failure",
            "tizi-branch-map-agnos-same-version",
            "agnos-model-manifest-boundary",
            "connectivity-thresholds"
        ]
    );
    assert_eq!(report["scenarios"].as_array().unwrap().len(), names.len());
    for name in names {
        let source: Value =
            serde_json::from_slice(&fs::read(output.join(format!("{name}-source.json"))).unwrap())
                .unwrap();
        let native: Value =
            serde_json::from_slice(&fs::read(output.join(format!("{name}-native.json"))).unwrap())
                .unwrap();
        assert!(assert_equivalent(&source, &native).unwrap() > 0);
        assert!(report["scenarios"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["scenario"] == name && row["passed"] == true));
    }
}

#[test]
fn comparison_accepts_only_exact_obsolete_staging_removal() {
    let obsolete = json!({"argv":["sudo","rm","-rf","<ROOT>/data/stage"],"cwd":null});
    let kept = json!({"argv":["git","fetch"],"cwd":"<ROOT>/base"});
    let source = json!({"commands":[obsolete.clone(),kept.clone()],"snapshots":[{"consistent":true}],"agnos":[]});
    let native = json!({"commands":[kept],"snapshots":[{"consistent":true}],"agnos":[]});
    assert_eq!(assert_equivalent(&source, &native).unwrap(), 1);
    let mut altered = native.clone();
    altered["snapshots"][0]["consistent"] = json!(false);
    assert!(assert_equivalent(&source, &altered).is_err());
    let mut altered = native.clone();
    altered["commands"] = json!([]);
    assert!(assert_equivalent(&source, &altered).is_err());
    let mut altered = source.clone();
    altered["commands"] =
        json!([{"argv":["sudo","rm","-rf","<ROOT>/data/stage/child"],"cwd":null}]);
    assert!(assert_equivalent(&altered, &native).is_err());
    assert!(assert_equivalent(&native, &native).is_err());
}

#[test]
fn python_integral_wait_equality_does_not_change_other_snapshot_fields() {
    let obsolete = json!({"argv":["sudo","rm","-rf","<ROOT>/data/stage"],"cwd":null});
    let source =
        json!({"commands":[obsolete],"snapshots":[{"wait":5400.0,"consistent":true}],"agnos":[]});
    let native = json!({"commands":[],"snapshots":[{"wait":5400,"consistent":true}],"agnos":[]});
    assert_eq!(assert_equivalent(&source, &native).unwrap(), 1);
    for wrong_wait in [json!(5399), json!(5401), json!("5400"), json!(null)] {
        let mut changed = native.clone();
        changed["snapshots"][0]["wait"] = wrong_wait;
        assert!(assert_equivalent(&source, &changed).is_err());
    }
    let mut changed = native;
    changed["snapshots"][0]["consistent"] = json!(false);
    assert!(assert_equivalent(&source, &changed).is_err());
}

#[test]
fn original_and_native_cli_reject_and_help_actions_match() {
    let receipts = required("UPDATED_CHECK_CLI_OUTPUT");
    fs::create_dir_all(&receipts).unwrap();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let original = repo.join("rust/tools/check_updated.py");
    assert_eq!(
        format!("{:x}", Sha256::digest(fs::read(&original).unwrap())),
        "bfa1686e464e5457b9a9bdc66b954e81f5994db24860d7f7b2869999f4e29b7d"
    );
    let python = required("UPDATED_CHECK_PYTHON");
    let native = PathBuf::from(env!("CARGO_BIN_EXE_check-updated"));
    let source_sha = format!("{:x}", Sha256::digest(fs::read(&original).unwrap()));
    let python_sha = format!("{:x}", Sha256::digest(fs::read(&python).unwrap()));
    let native_sha = format!("{:x}", Sha256::digest(fs::read(&native).unwrap()));
    for (index, (name, args)) in [
        ("help", vec!["--help"]),
        ("short-help-attached", vec!["-hfoo"]),
        ("short-help-repeated", vec!["-hh"]),
        (
            "repeated-binary-before-help",
            vec!["--binary", "first", "--binary", "second", "--help"],
        ),
        (
            "abbreviated-options-before-help",
            vec![
                "--bina", "b", "--lau", "l", "--bind", "p", "--out", "o", "--help",
            ],
        ),
        ("missing-binary-value", vec!["--binary"]),
        ("unknown-option", vec!["--unknown"]),
        ("ambiguous-binary-prefix", vec!["--bin"]),
        ("help-equals", vec!["--help=bad"]),
        ("short-help-equals", vec!["-h=bad"]),
        (
            "incomplete-required-set",
            vec!["--binary", "b", "--launcher", "l"],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let mut source_args = vec![
            python.as_os_str().to_owned(),
            original.as_os_str().to_owned(),
        ];
        source_args.extend(args.iter().map(OsString::from));
        let source = NativeRunner
            .run(&CommandSpec::new_os(
                source_args.clone(),
                Duration::from_secs(5),
            ))
            .unwrap();
        let mut native_args = vec![native.as_os_str().to_owned()];
        native_args.extend(args.iter().map(OsString::from));
        let candidate = NativeRunner
            .run(&CommandSpec::new_os(
                native_args.clone(),
                Duration::from_secs(5),
            ))
            .unwrap();
        let receipt = json!({
            "case":name,
            "source_checker_sha256":source_sha,
            "source_python_sha256":python_sha,
            "native_checker_sha256":native_sha,
            "source":{
                "argv":source_args.iter().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>(),
                "argv_hex":source_args.iter().map(|arg| hex(arg.as_bytes())).collect::<Vec<_>>(),
                "code":source.code,
                "stdout":String::from_utf8_lossy(&source.stdout),
                "stderr":String::from_utf8_lossy(&source.stderr),
                "stdout_hex":hex(&source.stdout),
                "stderr_hex":hex(&source.stderr),
            },
            "native":{
                "argv":native_args.iter().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>(),
                "argv_hex":native_args.iter().map(|arg| hex(arg.as_bytes())).collect::<Vec<_>>(),
                "code":candidate.code,
                "stdout":String::from_utf8_lossy(&candidate.stdout),
                "stderr":String::from_utf8_lossy(&candidate.stderr),
                "stdout_hex":hex(&candidate.stdout),
                "stderr_hex":hex(&candidate.stderr),
            }
        });
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(receipts.join(format!("{index:02}-{name}.json")))
            .unwrap();
        file.write_all(&serde_json::to_vec_pretty(&receipt).unwrap())
            .unwrap();
        file.write_all(b"\n").unwrap();
        file.sync_all().unwrap();
        assert_eq!(
            candidate.code,
            source.code,
            "CLI status for {args:?}: original stderr={} native stderr={}",
            String::from_utf8_lossy(&source.stderr),
            String::from_utf8_lossy(&candidate.stderr)
        );
        assert!(matches!(candidate.code, 0 | 2));
        if candidate.code == 0 {
            assert!(String::from_utf8_lossy(&candidate.stdout).contains("usage:"));
        }
    }
    assert_eq!(fs::read_dir(&receipts).unwrap().count(), 11);
    let parsed = parse(&[
        "--bina".into(),
        "first".into(),
        "--binary".into(),
        "last".into(),
        "--lau".into(),
        "launcher".into(),
        "--bind".into(),
        "binding".into(),
        "--out".into(),
        "output".into(),
    ])
    .unwrap()
    .unwrap();
    assert_eq!(parsed.binary, PathBuf::from("last"));
    assert_eq!(parsed.launcher, PathBuf::from("launcher"));
    assert_eq!(parsed.binding, PathBuf::from("binding"));
    assert_eq!(parsed.output, PathBuf::from("output"));
}

#[test]
fn preexisting_result_is_not_reused_as_pass() {
    let directory = tempfile::tempdir().unwrap();
    let receipt = directory.path().join("result.json");
    fs::write(&receipt, b"{\"passed\":true}\n").unwrap();
    let missing = directory.path().join("missing");
    assert!(run(
        &missing,
        &missing,
        &missing,
        directory.path(),
        &missing,
        &missing
    )
    .is_err());
    assert_eq!(fs::read(receipt).unwrap(), b"{\"passed\":true}\n");
}

#[test]
fn missing_candidate_cannot_leave_a_success_receipt() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("proof");
    let missing = directory.path().join("missing");
    assert!(run(&missing, &missing, &missing, &output, &missing, &missing).is_err());
    assert!(output.is_dir());
    assert!(!output.join("result.json").exists());
}

#[test]
fn failed_fresh_original_cannot_finalize_a_pass() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("failed-original");
    assert!(run(
        &required("UPDATED_CHECK_BINARY"),
        &required("UPDATED_CHECK_LAUNCHER"),
        &required("UPDATED_CHECK_BINDING"),
        &output,
        Path::new("/bin/false"),
        &required("UPDATED_CHECK_SHIM"),
    )
    .is_err());
    assert!(!output.join("result.json").exists());
    assert_eq!(
        fs::read(output.join("check-fetch-metered-source.stdout")).unwrap(),
        b"<empty>\n"
    );
}
