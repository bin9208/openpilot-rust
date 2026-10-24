//! Sealed original observation and exact-ELF checkout/absent retirement proof.
#![cfg(all(target_os = "linux", feature = "paramsd-loop-native"))]
use bzip2::read::BzDecoder;
use openpilot_dev_tools::{
    paramsd_loop_reference,
    qa_process::{CommandSpec, NativeRunner, Runner},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs,
    io::{self, Cursor, Read},
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
const SOURCES: [&str; 7] = [
    "rust/tools/paramsd_fixture.py",
    "rust/tools/paramsd_loop_source.py",
    "rust/crates/dev-tools/src/paramsd_loop_fixture_generate.rs",
    "rust/crates/dev-tools/src/paramsd_fixture_generate.rs",
    "rust/crates/dev-tools/src/paramsd_loop_reference.rs",
    "rust/crates/dev-tools/src/paramsd_loop_check.rs",
    "rust/crates/paramsd/examples/paramsd_loop.rs",
];
const OBSERVATION: &str = "08cdf1e60c73aeaff033514cf91f103fd20260036a621fcaab3f42eb172dcb78";
const SOURCE_RAW: &str = "6543996ddee69e2dfdd8314c4c8c08c73ca2c589bcdf8247272fc9f7e08f9a6b";
const SOURCE_PACKED: &str = "91b518dde68475cf8d346836957fe3d2a4a69641a430a85414fa9acf203c3245";
const INPUT_RAW: &str = "4f446bc98c9d303d292f487c9ebe36bc8721436a30627dfca55baabae4c6811d";
const INPUT_PACKED: &str = "6a5641f18914cbf452f027ed33daf64a372a966ebc89ae2a2e859a4de384f818";
const FIXTURE: &str = "tests/fixtures/paramsd-loop-retired-original";
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
fn original_raw(fixture: &Path, name: &str, packed: &str, raw: &str, bound: u64) -> Vec<u8> {
    let compressed = fs::read(fixture.join(format!("{name}.bz2"))).unwrap();
    assert_eq!(format!("{:x}", Sha256::digest(&compressed)), packed);
    let mut decoded = Vec::new();
    BzDecoder::new(Cursor::new(compressed))
        .take(bound + 1)
        .read_to_end(&mut decoded)
        .unwrap();
    assert!(decoded.len() as u64 <= bound);
    assert_eq!(format!("{:x}", Sha256::digest(&decoded)), raw);
    decoded
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
fn sealed_original_and_same_elf_native_checkout_absent_cover_all_loop_rows_without_python() {
    let repository =
        fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    let proof = tempfile::Builder::new()
        .prefix("retirement-")
        .tempdir_in(required("PARAMSD_LOOP_RETIREMENT_OUTPUT"))
        .unwrap()
        .keep();
    let candidate = required("PARAMSD_LOOP_NATIVE_CANDIDATE");
    let checker = fs::canonicalize(env!("CARGO_BIN_EXE_check-paramsd-loop-native")).unwrap();
    let mut seals = BTreeMap::new();
    for path in [&candidate, &checker] {
        seals.insert(path.to_str().unwrap().to_owned(), sha(path));
    }
    assert_eq!(seals.len(), 2);
    for (name, _) in RETIRED {
        missing(&repository.join(name));
    }
    let mut source_seals = BTreeMap::new();
    for name in SOURCES {
        source_seals.insert(name, sha(&repository.join(name)));
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

    let absent_root = proof.join("absent-root");
    for name in SOURCES {
        let dest = absent_root.join(name);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::copy(repository.join(name), &dest).unwrap();
        assert_eq!(sha(&dest), source_seals[name]);
    }
    for root in [&repository, &absent_root] {
        for (name, _) in RETIRED {
            missing(&root.join(name));
        }
    }

    assert_eq!(sha(&fixture.join("observation.json")), OBSERVATION);
    let observation = value(&fixture.join("observation.json"));
    assert_eq!(
        observation["checked_out_source"],
        "5f6104be47db0b3ae9131241726613788eeac747"
    );
    assert_eq!(observation["artifact"], 12317675555_u64);
    assert_eq!(observation["original_cli"]["exit_code"], 0);
    assert_eq!(observation["original_cli"]["timeout_seconds"], 600);
    for (name, digest) in RETIRED {
        assert_eq!(observation["original_python_source_sha256"][name], digest);
    }
    for (name, digest) in [
        (
            "original-status.json",
            "8f0c1c520d4343f54805dcdaa74287fb690f4d610186020a9343b1407654bdbd",
        ),
        (
            "original.stdout",
            "f265415b76afbc58c960229a5678a4ff0b16cb1c48e01ae0bf4ff76672dcb0bd",
        ),
        (
            "original.stderr",
            "12a3b8aa65146fa0a1ad99214c34ba53351b3525dedcbefc738e15967d3f79f5",
        ),
        (
            "process.log",
            "2a3954627795f50423fd88b74045fed1aa6fcc709aa93aa4113bc4d4ac5d9a30",
        ),
        (
            "results.json",
            "9d32312d8796bd69a25567310b8569ea90a786a6f3be1f4a7c2d099a938dc43d",
        ),
    ] {
        assert_eq!(sha(&fixture.join(name)), digest);
        assert_eq!(observation["original_only"][name], digest);
    }
    assert_eq!(value(&fixture.join("original-status.json"))["exit_code"], 0);
    assert_eq!(
        fs::read(fixture.join("process.log")).unwrap(),
        b"\nEXIT 0\n"
    );
    let original_receipt = value(&fixture.join("results.json"));
    assert_eq!(
        original_receipt,
        json!({"pass":true,"cases":39,"frames":2420,"saves":4,"gps_writes":216,"max_absolute_error":0.0})
    );
    let original_stdout: Value =
        serde_json::from_slice(&fs::read(fixture.join("original.stdout")).unwrap()).unwrap();
    assert_eq!(original_stdout, original_receipt);
    let original_source = original_raw(
        &fixture,
        "source.json",
        SOURCE_PACKED,
        SOURCE_RAW,
        8_000_000,
    );
    let source_path = proof.join("sealed-original-source.json");
    fs::write(&source_path, original_source).unwrap();
    // Only the three logged stack-address sequences in cases 9, 10, and 35
    // may differ from the older independent original observation.
    paramsd_loop_reference::verify_fresh_source(&source_path).unwrap();
    let original_input = original_raw(&fixture, "input.json", INPUT_PACKED, INPUT_RAW, 4_500_000);
    let original_request: Value = serde_json::from_slice(&original_input).unwrap();
    paramsd_loop_reference::verify_input(&original_request).unwrap();
    let original_cases = original_request["cases"].as_array().unwrap();
    assert_eq!(original_cases.len(), 39);
    let checker_text = checker.to_str().unwrap().to_owned();
    let candidate_text = candidate.to_str().unwrap().to_owned();
    let mut first_raw = None;
    for (lane, root) in [("checkout", &repository), ("absent", &absent_root)] {
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
        let raw = fs::read(output.join("native-raw.json")).unwrap();
        if let Some(previous) = &first_raw {
            assert_eq!(&raw, previous, "same ELF changed raw output across roots");
        } else {
            first_raw = Some(raw);
        }
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
        assert_eq!(historical.len(), 2);
        for (name, digest) in RETIRED {
            assert_eq!(historical.get(name).and_then(Value::as_str), Some(digest));
        }
        assert_eq!(
            value(&output.join("historical-sources.json")),
            report["historical_source_sha256"]
        );
        let live = report["sources"].as_array().unwrap();
        assert_eq!(live.len(), SOURCES.len() + 3);
        for (name, digest) in &source_seals {
            let source_path = root.join(name);
            let bound = live
                .iter()
                .find(|entry| entry["path"].as_str() == source_path.to_str());
            assert_eq!(bound.unwrap()["sha256"].as_str(), Some(digest.as_str()));
            assert_eq!(sha(&source_path), *digest);
        }
    }
    for (path, digest) in &seals {
        assert_eq!(sha(Path::new(path)), *digest);
    }
    for (name, digest) in &source_seals {
        assert_eq!(sha(&repository.join(name)), *digest);
    }
    for (name, _) in RETIRED {
        missing(&repository.join(name));
    }
    fs::write(proof.join("comparison.json"), serde_json::to_vec_pretty(&json!({"pass":true,"cases":39,"frames":2420,"saves":4,"gps_writes":216,"same_candidate_elf_in_all_lanes":true})).unwrap()).unwrap();
}
