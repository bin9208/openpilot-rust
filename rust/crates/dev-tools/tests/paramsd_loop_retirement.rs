//! Fresh original and exact-ELF native present/absent retirement proof.
#![cfg(all(target_os = "linux", feature = "paramsd-loop-native"))]
use openpilot_dev_tools::{
    paramsd_loop_reference,
    qa_process::{CommandSpec, NativeRunner, Runner},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

const RETIRED: [(&str, &str); 2] = [
    (
        "rust/tools/check_paramsd_loop.py",
        "56c9afe7bc9bb21220478f21405ebf5327a931b0403c7cf628203e0b49fd58c3",
    ),
    (
        "rust/tools/paramsd_loop_fixture.py",
        "767f58edf7163edf8eaa493b5453f3bc00f6966772dba932e96a294e3efe3089",
    ),
];
const SOURCES: [&str; 9] = [
    "rust/tools/check_paramsd_loop.py",
    "rust/tools/paramsd_loop_fixture.py",
    "rust/tools/paramsd_fixture.py",
    "rust/tools/paramsd_loop_source.py",
    "rust/crates/dev-tools/src/paramsd_loop_fixture_generate.rs",
    "rust/crates/dev-tools/src/paramsd_fixture_generate.rs",
    "rust/crates/dev-tools/src/paramsd_loop_reference.rs",
    "rust/crates/dev-tools/src/paramsd_loop_check.rs",
    "rust/crates/paramsd/examples/paramsd_loop.rs",
];
fn required(name: &str) -> PathBuf {
    fs::canonicalize(
        std::env::var_os(name).unwrap_or_else(|| panic!("mandatory cloud prerequisite: {name}")),
    )
    .unwrap()
}
fn sha(path: &Path) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}
fn value(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn missing(path: &Path) {
    assert!(matches!(fs::symlink_metadata(path), Err(e) if e.kind() == io::ErrorKind::NotFound));
}
fn audited_exec(trace: &Path) -> Vec<(String, Vec<String>)> {
    let opening = regex::Regex::new(r"\bexecve(?:at)?\(").unwrap();
    let invocation =
        regex::Regex::new(r#"\bexecve\(("(?:[^"\\]|\\.)*"), (\[.*\]), .*\) = 0$"#).unwrap();
    let mut observed = Vec::new();
    for line in fs::read_to_string(trace).unwrap().lines() {
        assert!(
            !line.contains("execve resumed") && !line.contains("execveat resumed"),
            "resumed: {line}"
        );
        if !opening.is_match(line) {
            continue;
        }
        let matched = invocation
            .captures(line)
            .unwrap_or_else(|| panic!("failed/opaque/execveat: {line}"));
        observed.push((
            serde_json::from_str::<String>(&matched[1]).unwrap(),
            serde_json::from_str::<Vec<String>>(&matched[2]).unwrap(),
        ));
    }
    observed
}
fn native_spec(
    checker: &Path,
    candidate: &Path,
    output: &Path,
    root: &Path,
    trace: &Path,
) -> CommandSpec {
    let mut spec = CommandSpec::new_os(
        vec![
            "/usr/bin/strace".into(),
            "-f".into(),
            "-qq".into(),
            "-s".into(),
            "65535".into(),
            "-e".into(),
            "trace=execve,execveat".into(),
            "-o".into(),
            trace.as_os_str().into(),
            checker.as_os_str().into(),
            "--binary".into(),
            candidate.as_os_str().into(),
            "--evidence".into(),
            output.as_os_str().into(),
        ],
        Duration::from_secs(240),
    );
    spec.set_os_env(BTreeMap::from([
        (OsString::from("PATH"), OsString::from("/nonexistent")),
        (
            OsString::from("OPENPILOT_RUST_ROOT"),
            root.as_os_str().into(),
        ),
    ]));
    spec.env_remove = std::env::vars_os()
        .map(|(name, _)| name)
        .filter(|name| {
            name.as_os_str() != OsStr::new("PATH")
                && name.as_os_str() != OsStr::new("OPENPILOT_RUST_ROOT")
        })
        .collect();
    spec
}

#[test]
fn original_and_same_elf_native_present_absent_cover_all_loop_rows_without_python() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let proof = tempfile::Builder::new()
        .prefix("retirement-")
        .tempdir_in(required("PARAMSD_LOOP_RETIREMENT_OUTPUT"))
        .unwrap()
        .keep();
    let candidate = required("PARAMSD_LOOP_NATIVE_CANDIDATE");
    let oracle = required("PARAMSD_LOOP_ORACLE");
    let python = required("PARAMSD_LOOP_ORIGINAL_PYTHON");
    let checker = fs::canonicalize(env!("CARGO_BIN_EXE_check-paramsd-loop-native")).unwrap();
    let mut seals = BTreeMap::new();
    for path in [&candidate, &checker, &python] {
        seals.insert(path.to_str().unwrap().to_owned(), sha(path));
    }
    assert_eq!(seals.len(), 3);
    let mut source_seals = BTreeMap::new();
    for name in SOURCES {
        source_seals.insert(name, sha(&repository.join(name)));
    }
    for (name, digest) in RETIRED {
        assert_eq!(source_seals[name], digest);
    }
    assert_eq!(
        source_seals["rust/tools/paramsd_fixture.py"],
        "8c0356bb780039c475def68358567489e036ab422921b8389bc7fbed20635521"
    );
    assert_eq!(
        source_seals["rust/tools/paramsd_loop_source.py"],
        "4718a02dd4d43f54a5254eb3a5d0999355e5f46a669e2a8586ddf91929f3fca0"
    );
    fs::write(
        proof.join("inputs-before.json"),
        serde_json::to_vec_pretty(&json!({"executables":seals,"source_sha256":source_seals}))
            .unwrap(),
    )
    .unwrap();

    let present_root = proof.join("present-root");
    let absent_root = proof.join("absent-root");
    for private in [&present_root, &absent_root] {
        for name in SOURCES {
            let dest = private.join(name);
            fs::create_dir_all(dest.parent().unwrap()).unwrap();
            fs::copy(repository.join(name), &dest).unwrap();
            assert_eq!(sha(&dest), source_seals[name]);
        }
    }
    for (name, _) in RETIRED {
        fs::remove_file(absent_root.join(name)).unwrap();
        missing(&absent_root.join(name));
    }

    let original = proof.join("original");
    let mut original_argv = vec![
        python.clone().into(),
        "-P".into(),
        repository
            .join("rust/tools/check_paramsd_loop.py")
            .into_os_string(),
    ];
    original_argv.extend([
        "--oracle".into(),
        oracle.clone().into_os_string(),
        "--trace".into(),
        candidate.clone().into_os_string(),
        "--evidence".into(),
        original.clone().into_os_string(),
    ]);
    let mut original_spec = CommandSpec::new_os(original_argv, Duration::from_secs(600));
    let mut python_paths = vec![
        repository.clone(),
        repository.join("rust/tools"),
        repository.join("opendbc_repo"),
    ];
    if let Some(inherited) = std::env::var_os("PYTHONPATH") {
        python_paths.extend(std::env::split_paths(&inherited));
    }
    original_spec.env.insert(
        "PYTHONPATH".into(),
        std::env::join_paths(python_paths)
            .unwrap()
            .to_str()
            .unwrap()
            .into(),
    );
    let source_status = NativeRunner
        .run_to_files(
            &original_spec,
            &proof.join("original.stdout"),
            &proof.join("original.stderr"),
        )
        .unwrap();
    fs::write(
        proof.join("original-status.json"),
        serde_json::to_vec_pretty(&json!({"exit_code":source_status.code})).unwrap(),
    )
    .unwrap();
    assert_eq!(
        source_status.code,
        0,
        "{}",
        String::from_utf8_lossy(&fs::read(proof.join("original.stderr")).unwrap())
    );
    let original_request = value(&original.join("input.json"));
    paramsd_loop_reference::verify_input(&original_request).unwrap();
    let original_cases = original_request["cases"].as_array().unwrap();
    assert_eq!(original_cases.len(), 39);
    let original_raw = original.join("native-raw.json");
    assert_eq!(
        sha(&original.join("source.json")),
        "23d374139b2430bd8da0eb800c0a8a91900c8d08c0df90373ba2f4d7419ae22a"
    );
    assert_eq!(
        sha(&original.join("input.json")),
        "4f446bc98c9d303d292f487c9ebe36bc8721436a30627dfca55baabae4c6811d"
    );
    let original_summary = paramsd_loop_reference::compare_file(&original_raw).unwrap();
    assert_eq!(
        (
            original_summary.cases,
            original_summary.frames,
            original_summary.saves,
            original_summary.gps_writes
        ),
        (39, 2420, 4, 216)
    );

    let checker_text = checker.to_str().unwrap().to_owned();
    let candidate_text = candidate.to_str().unwrap().to_owned();
    for (lane, root) in [("present", &present_root), ("absent", &absent_root)] {
        let output = proof.join(format!("native-{lane}"));
        let trace = proof.join(format!("native-{lane}.exec"));
        let spec = native_spec(&checker, &candidate, &output, root, &trace);
        let status = NativeRunner
            .run_to_files(
                &spec,
                &proof.join(format!("native-{lane}.stdout")),
                &proof.join(format!("native-{lane}.stderr")),
            )
            .unwrap();
        fs::write(
            proof.join(format!("native-{lane}-status.json")),
            serde_json::to_vec_pretty(&json!({"exit_code":status.code})).unwrap(),
        )
        .unwrap();
        assert_eq!(
            status.code,
            0,
            "{lane}: {}",
            String::from_utf8_lossy(
                &fs::read(proof.join(format!("native-{lane}.stderr"))).unwrap()
            )
        );
        let output_text = output.to_str().unwrap().to_owned();
        let expected = vec![
            (
                checker_text.clone(),
                vec![
                    checker_text.clone(),
                    "--binary".into(),
                    candidate_text.clone(),
                    "--evidence".into(),
                    output_text.clone(),
                ],
            ),
            (
                candidate_text.clone(),
                vec![
                    candidate_text.clone(),
                    format!("{output_text}/native-raw.json"),
                ],
            ),
        ];
        let measured = audited_exec(&trace);
        fs::write(
            proof.join(format!("native-{lane}-exec-audit.json")),
            serde_json::to_vec_pretty(
                &json!({"measured":&measured,"expected":&expected,"sealed_executables_sha256":&seals}),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            measured, expected,
            "{lane}: unexpected native executable or argv"
        );
        assert_eq!(
            value(&output.join("input.json")),
            original_request,
            "{lane} complete request"
        );
        assert_eq!(
            fs::read(output.join("native-raw.json")).unwrap(),
            fs::read(&original_raw).unwrap(),
            "{lane} raw output"
        );
        let summary =
            paramsd_loop_reference::compare_file(&output.join("native-raw.json")).unwrap();
        assert_eq!(
            (
                summary.cases,
                summary.frames,
                summary.saves,
                summary.gps_writes
            ),
            (39, 2420, 4, 216)
        );
        let report = value(&output.join("result.json"));
        assert_eq!(report["result"], "pass");
        assert_eq!(
            (
                report["cases"].as_u64(),
                report["frames"].as_u64(),
                report["saves"].as_u64(),
                report["gps_writes"].as_u64()
            ),
            (Some(39), Some(2420), Some(4), Some(216))
        );
        let historical = report["historical_source_sha256"].as_object().unwrap();
        assert_eq!(historical.len(), if lane == "absent" { 2 } else { 0 });
        for (name, digest) in RETIRED {
            assert_eq!(
                historical.get(name).and_then(Value::as_str),
                (lane == "absent").then_some(digest)
            );
        }
        assert_eq!(
            value(&output.join("historical-sources.json")),
            report["historical_source_sha256"]
        );
        let live = report["sources"].as_array().unwrap();
        assert_eq!(live.len(), SOURCES.len() + 3 - historical.len());
        for (name, digest) in &source_seals {
            let source_path = root.join(name);
            let bound = live
                .iter()
                .find(|entry| entry["path"].as_str() == source_path.to_str());
            if lane == "absent" && RETIRED.iter().any(|(retired, _)| retired == name) {
                assert!(bound.is_none(), "absent source retained as live: {name}");
                missing(&source_path);
            } else {
                assert_eq!(bound.unwrap()["sha256"].as_str(), Some(digest.as_str()));
                assert_eq!(sha(&source_path), *digest);
            }
        }
    }
    for (path, digest) in &seals {
        assert_eq!(sha(Path::new(path)), *digest);
    }
    for (name, digest) in &source_seals {
        assert_eq!(sha(&repository.join(name)), *digest);
    }
    fs::write(proof.join("comparison.json"), serde_json::to_vec_pretty(&json!({"pass":true,"cases":39,"frames":2420,"saves":4,"gps_writes":216,"same_candidate_elf_in_all_lanes":true})).unwrap()).unwrap();
}
