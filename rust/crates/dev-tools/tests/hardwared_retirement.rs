//! Hosted actual CLI and independent source-absent checkout closure.
#![cfg(all(target_os = "linux", feature = "hardwared-qa-native"))]
use openpilot_dev_tools::{
    hardwared_capture,
    qa_process::{CommandSpec, NativeRunner, Runner},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs,
    os::unix::ffi::OsStrExt,
    path::{Component, Path, PathBuf},
    time::Duration,
};
const RETIRED: [&str; 3] = [
    "rust/tools/check_hardwared.py",
    "rust/tools/hardwared_source.py",
    "rust/tools/check_hardwared_daemon.py",
];
const FIXTURE: &str = "rust/crates/dev-tools/tests/fixtures/hardwared-original";
fn required(name: &str) -> PathBuf {
    std::env::var_os(name)
        .unwrap_or_else(|| panic!("{name} required"))
        .into()
}
fn write(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn absence(root: &Path) {
    for name in RETIRED {
        assert!(
            fs::symlink_metadata(root.join(name))
                .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
            "retired source present: {name}"
        );
    }
}
fn invoke(checker: &Path, args: &[OsString], cwd: &Path, output: &Path, expected: i32) {
    fs::create_dir(output).unwrap();
    let argv = std::iter::once(checker.as_os_str().to_owned())
        .chain(args.iter().cloned())
        .collect::<Vec<_>>();
    let mut spec = CommandSpec::new_os(argv.clone(), Duration::from_secs(300));
    spec.cwd = Some(cwd.into());
    spec.env_remove = vec![
        "PYTHONPATH".into(),
        "PYTHONHOME".into(),
        "ZMQ".into(),
        "CEREAL_FAKE".into(),
        "OPENPILOT_PREFIX".into(),
    ];
    spec.stdout_limit = 8 * 1024 * 1024;
    spec.stderr_limit = 8 * 1024 * 1024;
    let result = NativeRunner.run_to_files(
        &spec,
        &output.join("stdout.log"),
        &output.join("stderr.log"),
    );
    write(
        &output.join("invocation.json"),
        &json!({"argv_bytes":argv.iter().map(|a|a.as_bytes().to_vec()).collect::<Vec<_>>(),"cwd":cwd,"timeout_seconds":300,"exit_code":result.as_ref().ok().map(|c|c.code),"error":result.as_ref().err().map(ToString::to_string)}),
    );
    let result = result.unwrap();
    assert_eq!(
        result.code,
        expected,
        "{}",
        fs::read_to_string(output.join("stderr.log")).unwrap()
    );
}
fn args(
    source: &Path,
    binary: &Path,
    output: &Path,
    daemon: bool,
    launcher: &Path,
    retired: bool,
) -> Vec<OsString> {
    let mut args = if daemon {
        vec![
            "--binary".into(),
            binary.into(),
            "--output".into(),
            output.into(),
            "--launcher".into(),
            launcher.into(),
        ]
    } else {
        vec![binary.into(), output.into()]
    };
    args.extend([OsString::from("--source-root"), source.into()]);
    if retired {
        args.push("--require-retired".into());
    }
    args
}
#[test]
#[ignore = "hosted full original hardwared capture, actual native CLIs/daemon/trace/launcher required"]
fn same_native_clis_preserve_all_original_cases_and_live_ipc_in_both_roots() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let output = required("HARDWARED_RETIREMENT_OUTPUT");
    fs::create_dir(&output).unwrap();
    let trace = required("HARDWARED_TRACE_BINARY").canonicalize().unwrap();
    let daemon = required("HARDWARED_DAEMON_BINARY").canonicalize().unwrap();
    let launcher = required("HARDWARED_PROCESS_LAUNCHER")
        .canonicalize()
        .unwrap();
    let checkers = [
        PathBuf::from(env!("CARGO_BIN_EXE_check-hardwared")),
        PathBuf::from(env!("CARGO_BIN_EXE_check-hardwared-daemon")),
    ];
    let retired = std::env::var_os("HARDWARED_REQUIRE_RETIRED").is_some_and(|v| v == "1");
    if retired {
        absence(&root);
    }
    let manifest: Value = serde_json::from_slice(
        &fs::read(root.join(FIXTURE).join("capture-manifest.json")).unwrap(),
    )
    .unwrap();
    let private = output.join("private-source");
    fs::create_dir(&private).unwrap();
    fs::create_dir_all(private.join("rust/tools")).unwrap();
    let mut copied = Vec::new();
    for (name, pin) in manifest["source_pins"].as_object().unwrap() {
        if RETIRED.contains(&name.as_str()) {
            continue;
        }
        assert!(Path::new(name)
            .components()
            .all(|c| matches!(c, Component::Normal(_))));
        let raw = fs::read(root.join(name)).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&raw)),
            pin["sha256"].as_str().unwrap()
        );
        assert_eq!(raw.len() as u64, pin["bytes"].as_u64().unwrap());
        let target = private.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, &raw).unwrap();
        assert_eq!(fs::read(target).unwrap(), raw);
        copied.push(json!({"path":name,"pin":pin}));
    }
    assert_eq!(copied.len(), 16);
    fs::create_dir_all(private.join(FIXTURE)).unwrap();
    for name in ["capture-manifest.json", "source.json.gz", "README.md"] {
        let raw = fs::read(root.join(FIXTURE).join(name)).unwrap();
        fs::write(private.join(FIXTURE).join(name), raw).unwrap();
    }
    absence(&private);
    write(
        &output.join("private-source-receipt.json"),
        &json!({"copied_sources":copied,"retired_absent":RETIRED}),
    );
    let empty = output.join("empty-cwd");
    fs::create_dir(&empty).unwrap();
    let mut receipts = Vec::new();
    for source in [&root, &private] {
        for (checker, binary) in checkers.iter().zip([&trace, &daemon]) {
            receipts.push(
                hardwared_capture::validate(
                    source,
                    &source.join(FIXTURE),
                    checker,
                    &[],
                    Some(binary),
                    source == &private || retired,
                )
                .unwrap(),
            );
        }
        receipts.push(
            hardwared_capture::validate(
                source,
                &source.join(FIXTURE),
                &launcher,
                &[],
                None,
                source == &private || retired,
            )
            .unwrap(),
        );
    }
    write(
        &output.join("held-before.json"),
        &json!(receipts.iter().map(|r| r.report()).collect::<Vec<_>>()),
    );
    for (label, source, cwd) in [("actual", &root, &root), ("private", &private, &empty)] {
        for (index, binary) in [&trace, &daemon].into_iter().enumerate() {
            let case = output.join(format!("{label}-{index}"));
            let argv = args(
                source,
                binary,
                &case,
                index == 1,
                &launcher,
                source == &private || retired,
            );
            invoke(
                &checkers[index],
                &argv,
                cwd,
                &output.join(format!("{label}-{index}-invocation")),
                0,
            );
            let report: Value =
                serde_json::from_slice(&fs::read(case.join("result.json")).unwrap()).unwrap();
            let stdout: Value = serde_json::from_slice(
                &fs::read(output.join(format!("{label}-{index}-invocation/stdout.log"))).unwrap(),
            )
            .unwrap();
            assert_eq!(
                stdout, report,
                "actual CLI output must match retained report"
            );
            assert_eq!(report["result"], "PASS");
            if index == 0 {
                assert_eq!(report["case_count"], 12);
                assert_eq!(report["observation_count"], 890);
            } else {
                assert_eq!(report["exit_code"], 0);
                assert_eq!(report["peer"]["transitions"].as_array().unwrap().len(), 5);
            }
            for receipt in &receipts {
                receipt.verify().unwrap();
            }
        }
        for index in 0..2 {
            for (name, argv, code) in [
                ("help", vec![OsString::from("--help")], 0),
                ("missing", vec![], 2),
                ("unknown", vec![OsString::from("--unknown")], 2),
            ] {
                invoke(
                    &checkers[index],
                    &argv,
                    cwd,
                    &output.join(format!("{label}-{index}-{name}")),
                    code,
                );
            }
        }
    }
    let configured_output = output.join("configured-runner");
    let mut configured = args(&private, &trace, &configured_output, false, &launcher, true);
    configured.extend([
        OsString::from("--runner"),
        PathBuf::from("/usr/bin/env")
            .canonicalize()
            .unwrap()
            .into_os_string(),
        OsString::from("--runner=-u"),
        OsString::from("--runner=OPENPILOT_PREFIX"),
    ]);
    invoke(
        &checkers[0],
        &configured,
        &empty,
        &output.join("configured-runner-invocation"),
        0,
    );
    let configured_report: Value =
        serde_json::from_slice(&fs::read(configured_output.join("result.json")).unwrap()).unwrap();
    assert_eq!(configured_report["observation_count"], 890);
    for (index, binary) in [&trace, &daemon].into_iter().enumerate() {
        for (n, name) in RETIRED.iter().enumerate() {
            let target = private.join(name);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(&target, b"retired source reappearance").unwrap();
            invoke(
                &checkers[index],
                &args(
                    &private,
                    binary,
                    &output.join(format!("reappearance-uncreated-{index}-{n}")),
                    index == 1,
                    &launcher,
                    true,
                ),
                &empty,
                &output.join(format!("reappearance-{index}-{n}")),
                1,
            );
            fs::remove_file(target).unwrap();
            assert!(!output
                .join(format!("reappearance-uncreated-{index}-{n}"))
                .exists());
        }
    }
    absence(&private);
    for receipt in &receipts {
        receipt.verify().unwrap();
    }
    write(
        &output.join("held-after.json"),
        &json!(receipts.iter().map(|r| r.report()).collect::<Vec<_>>()),
    );
    write(
        &output.join("summary.json"),
        &json!({"result":"PASS","actual_require_retired":retired,"private_require_retired":true,"roots":2,"trace_cases_per_root":12,"trace_observations_per_root":890,"live_ipc_roots":2,"configured_runner_trace_cases":12,"cli_admissions":12,"source_reappearance_controls":6,"device_validation":false}),
    );
}
