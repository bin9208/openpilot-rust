//! Maintained native startup caller and private-source-absence audit. The first
//! independent original/native gate must be accepted before this caller moves.
#![cfg(all(target_os = "linux", feature = "paramsd-startup-check-native"))]
use capnp::{message::ReaderOptions, serialize};
use openpilot_cereal::log_capnp::event;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

const ORIGINAL: &[u8] = include_bytes!("fixtures/paramsd-startup-original-r3/observation.json");
const ORIGINAL_SHA256: &str = "e673266375d15d557928b7e0c6c7bfb63f286045bf901313a1496c47e71ea4ca";
fn corpus() -> Value {
    assert_eq!(format!("{:x}", Sha256::digest(ORIGINAL)), ORIGINAL_SHA256);
    let original: Value = serde_json::from_slice(ORIGINAL).unwrap();
    assert_eq!(
        original["source"],
        "69bbee211a5bfde590ab9d85fb071d967892b580"
    );
    assert_eq!(original["run"], 39834626379_u64);
    assert_eq!(
        original["archive_sha256"],
        "93089e39039b72c15623481c8d7f0a88a6ac418302dbc3c3477230cccafdbff3"
    );
    original
}

#[test]
fn accepted_startup_corpus_binds_original_seven_calls_and_saved_files() {
    let original = corpus();
    assert_eq!(
        original["inputs"],
        serde_json::to_value(openpilot_dev_tools::paramsd_startup_fixture::cases()).unwrap()
    );
    let snapshots = original["snapshots"].as_array().unwrap();
    assert_eq!(snapshots.len(), 7);
    for (index, snapshot) in snapshots.iter().enumerate() {
        let name = snapshot["name"].as_str().unwrap();
        assert_eq!(original["results"]["scenarios"][index]["scenario"], name);
        let params = snapshot["params"].as_object().unwrap();
        let keys = params.keys().cloned().collect::<Vec<_>>();
        assert_eq!(
            serde_json::to_value(keys).unwrap(),
            original["results"]["scenarios"][index]["files"]
        );
        for (key, bytes) in params {
            let bytes: Vec<u8> = serde_json::from_value(bytes.clone()).unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(bytes)),
                original["original_member_sha256"][format!("{name}/params/{key}")]
            );
        }
        if !snapshot["gps"].is_null() {
            let bytes: Vec<u8> = serde_json::from_value(snapshot["gps"].clone()).unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(bytes)),
                original["original_member_sha256"][format!("{name}/LastGPSPosition.bin")]
            );
        }
    }
}

fn required(name: &str) -> PathBuf {
    fs::canonicalize(
        std::env::var_os(name).unwrap_or_else(|| panic!("mandatory hosted evidence: {name}")),
    )
    .unwrap()
}
fn value(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn sha(path: &Path) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}
fn absent(path: &Path) {
    assert!(
        matches!(fs::symlink_metadata(path), Err(error) if error.kind() == io::ErrorKind::NotFound),
        "source reappeared: {}",
        path.display()
    );
}
fn migration(bytes: &[u8]) -> Vec<u8> {
    let mut input = bytes;
    let message = serialize::read_message(&mut input, ReaderOptions::new()).unwrap();
    assert!(input.is_empty(), "trailing migration bytes");
    let root = message.get_root::<event::Reader<'_>>().unwrap();
    assert!(root.get_log_mono_time() > 0 && !root.get_valid());
    assert!(matches!(root.which().unwrap(), event::LiveParameters(_)));
    // Preserve the complete Event, including unknown scalar bits/pointers.
    // Only independent runtime monotonic time is normalized after decoding.
    let mut normalized = capnp::message::Builder::new_default();
    normalized.set_root(root).unwrap();
    normalized
        .get_root::<event::Builder<'_>>()
        .unwrap()
        .set_log_mono_time(0);
    serialize::write_message_to_words(&normalized)
}
fn execs(trace: &Path) -> Vec<(String, Vec<String>)> {
    let opening = regex::Regex::new(r"\bexecve(?:at)?\(").unwrap();
    let invocation =
        regex::Regex::new(r#"\bexecve\(("(?:[^"\\]|\\.)*"), (\[.*\]), .*\) = 0$"#).unwrap();
    let mut found = Vec::new();
    for line in fs::read_to_string(trace).unwrap().lines() {
        assert!(
            !line.contains("execve resumed") && !line.contains("execveat resumed"),
            "opaque exec: {line}"
        );
        if opening.is_match(line) {
            let fields = invocation
                .captures(line)
                .unwrap_or_else(|| panic!("extra/failed/opaque exec: {line}"));
            found.push((
                serde_json::from_str(&fields[1]).unwrap(),
                serde_json::from_str(&fields[2]).unwrap(),
            ));
        }
    }
    found
}

#[test]
#[ignore = "hosted maintained two-root startup caller audit only"]
fn native_startup_caller_preserves_seven_cases_and_private_source_absence() {
    let original_observation = corpus();
    let proof = required("PARAMSD_STARTUP_CALLER_PROOF");
    let root = required("PARAMSD_STARTUP_CALLER_ROOT");
    let private = required("PARAMSD_STARTUP_CALLER_ABSENT_ROOT");
    let checker = required("PARAMSD_STARTUP_CALLER_CHECKER");
    let daemon = required("PARAMSD_STARTUP_CALLER_DAEMON");
    let original = "rust/tools/check_paramsd_startup.py";
    assert_eq!(
        sha(&root.join(original)),
        "99d5d76b292d841c9d0e8b78a29e8750679aa297ed0dfea473d829e3877c22eb"
    );
    absent(&private.join(original));
    let cases = [
        "blocked-car",
        "malformed-car",
        "migration",
        "invalid-json",
        "wrong-fields",
        "malformed-cache",
        "wrong-covariance-dimensions",
    ];
    let mut reports = Vec::new();
    let mut inputs = Vec::new();
    for (index, source) in [&root, &private].into_iter().enumerate() {
        let lane = proof.join(format!("native-{index}"));
        let report = value(&lane.join("results.json"));
        assert_eq!(report["pass"], true);
        assert_eq!(
            report["scenarios"],
            original_observation["results"]["scenarios"]
        );
        assert_eq!(
            value(&lane.join("inputs.json")),
            original_observation["inputs"]
        );
        assert_eq!(report["startup_original_absent"], index == 1);
        assert_eq!(report["startup_seconds"], 10);
        assert_eq!(report["shutdown_seconds"], 5);
        assert_eq!(report["scenarios"].as_array().unwrap().len(), 7);
        let mut expected_exec = vec![(
            checker.to_str().unwrap().to_owned(),
            vec![
                checker.to_str().unwrap().into(),
                "--root".into(),
                source.to_str().unwrap().into(),
                "--binary".into(),
                daemon.to_str().unwrap().into(),
                "--evidence".into(),
                lane.to_str().unwrap().into(),
            ],
        )];
        for (case_index, case) in cases.iter().enumerate() {
            let output = lane.join(case);
            let saved = &original_observation["snapshots"][case_index];
            assert_eq!(saved["name"], *case);
            for (key, bytes) in saved["params"].as_object().unwrap() {
                let expected: Vec<u8> = serde_json::from_value(bytes.clone()).unwrap();
                let actual = fs::read(output.join("params").join(key)).unwrap();
                if *case == "migration" && key == "LiveParametersV2" {
                    assert_eq!(migration(&expected), migration(&actual));
                } else {
                    assert_eq!(expected, actual);
                }
            }
            let gps = output.join("LastGPSPosition.bin");
            assert_eq!(gps.exists(), !saved["gps"].is_null());
            if gps.exists() {
                assert_eq!(
                    fs::read(gps).unwrap(),
                    serde_json::from_value::<Vec<u8>>(saved["gps"].clone()).unwrap()
                );
            }
            assert_eq!(report["scenarios"][case_index]["scenario"], *case);
            assert_eq!(
                value(&output.join("results.json")),
                report["scenarios"][case_index]
            );
            let process = value(&output.join("process.json"));
            let argv: Vec<String> = serde_json::from_value(process["argv"].clone()).unwrap();
            assert_eq!(argv.len(), 3);
            assert_eq!(argv[0], daemon.to_str().unwrap());
            assert_eq!(argv[1], "--memory-root");
            expected_exec.push((daemon.to_str().unwrap().into(), argv));
            let pid = process["pid"].as_u64().unwrap();
            assert!(
                !Path::new(&format!("/proc/{pid}")).exists(),
                "unreaped startup child"
            );
            assert!(!Path::new("/dev/shm")
                .join(format!("msgq_{}", process["prefix"].as_str().unwrap()))
                .exists());
            assert!(!fs::read_dir(&output).unwrap().any(|entry| entry
                .unwrap()
                .file_name()
                .as_encoded_bytes()
                .starts_with(b"private-")));
            let lifecycle = value(&output.join("lifecycle.json"));
            if matches!(*case, "malformed-car" | "wrong-covariance-dimensions") {
                assert_eq!(lifecycle["exit"], 1);
                assert!(lifecycle["signal"].is_null() && lifecycle["shutdown_seconds"].is_null());
            } else {
                assert_eq!(lifecycle["exit"], 0);
                assert_eq!(lifecycle["signal"], "SIGTERM");
                assert!((0.0..5.0).contains(&lifecycle["shutdown_seconds"].as_f64().unwrap()));
                let maps = fs::read_to_string(output.join("maps.txt")).unwrap();
                assert!(maps.contains(daemon.to_str().unwrap()));
                assert!(!["libpython", "ekf_sym_pyx", "libcar.so"]
                    .iter()
                    .any(|needle| maps.contains(needle)));
            }
        }
        assert_eq!(
            execs(&proof.join(format!("native-{index}.exec"))),
            expected_exec
        );
        for binding in report["bindings"].as_array().unwrap() {
            assert_eq!(
                sha(Path::new(binding["path"].as_str().unwrap())),
                binding["sha256"].as_str().unwrap()
            );
        }
        inputs.push(value(&lane.join("inputs.json")));
        reports.push(report["scenarios"].clone());
    }
    assert_eq!(inputs[0], inputs[1]);
    assert_eq!(reports[0], reports[1]);
    for case in cases {
        let left = proof.join("native-0").join(case);
        let right = proof.join("native-1").join(case);
        let mut keys = fs::read_dir(left.join("params"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        keys.sort();
        let mut other = fs::read_dir(right.join("params"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        other.sort();
        assert_eq!(keys, other);
        for key in keys {
            let a = fs::read(left.join("params").join(&key)).unwrap();
            let b = fs::read(right.join("params").join(&key)).unwrap();
            if case == "migration" && key == "LiveParametersV2" {
                assert_eq!(migration(&a), migration(&b));
                assert_eq!(a, fs::read(left.join("migrated.bin")).unwrap());
                assert_eq!(b, fs::read(right.join("migrated.bin")).unwrap());
            } else {
                assert_eq!(a, b);
            }
        }
        let a = left.join("LastGPSPosition.bin");
        let b = right.join("LastGPSPosition.bin");
        assert_eq!(a.exists(), b.exists());
        if a.exists() {
            assert_eq!(fs::read(a).unwrap(), fs::read(b).unwrap());
        }
    }
    absent(&private.join(original));
    fs::write(
        proof.join("caller-audit.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "pass":true,"cases_per_lane":7,"execs_per_lane":8,"python_execs":0,
            "checkout_original_absent":false,"private_original_absent":true,
            "retired_sources":[],"device_acceptance":false
        }))
        .unwrap(),
    )
    .unwrap();
}
