//! Mandatory independent cloud pair. No original/candidate is run by this audit.
#![cfg(all(target_os = "linux", feature = "logmessaged-startup-native"))]
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};
fn json(path: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn case_order(bytes: &str) -> Vec<String> {
    let value = JsonValue::parse(bytes).unwrap();
    match value.get("cases").unwrap().view() {
        JsonView::Object(fields) => fields
            .into_iter()
            .map(|(key, _)| key.iter().map(|v| char::from_u32(*v).unwrap()).collect())
            .collect(),
        _ => panic!("cases must be an ordered object"),
    }
}
fn disk(output: &Path, case: &str) -> Vec<Value> {
    let mut rows = vec![];
    for directory in fs::read_dir(output.join(case).join("collector/home")).unwrap() {
        let directory = directory.unwrap();
        if !directory
            .file_name()
            .as_encoded_bytes()
            .starts_with(b".comma")
        {
            continue;
        }
        for file in fs::read_dir(directory.path().join("log")).unwrap() {
            let file = file.unwrap();
            if file.file_name().as_encoded_bytes().starts_with(b"swaglog.") {
                rows.extend(
                    fs::read_to_string(file.path())
                        .unwrap()
                        .lines()
                        .map(|line| serde_json::from_str::<Value>(line).unwrap()),
                );
            }
        }
    }
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["msg$s"], "first-error-after-start");
    for row in &mut rows {
        let id = row.as_object_mut().unwrap().remove("id").unwrap();
        let id = id.as_str().expect("UUID is a string");
        let plain = id.replace('-', "");
        assert_eq!(plain.len(), 32);
        assert!(plain.bytes().all(|v| v.is_ascii_hexdigit()));
        if id.len() != 32 {
            assert_eq!(id.len(), 36);
            for at in [8, 13, 18, 23] {
                assert_eq!(id.as_bytes()[at], b'-');
            }
        }
        assert_eq!(plain.as_bytes()[12].to_ascii_lowercase(), b'4');
        assert!(matches!(
            plain.as_bytes()[16].to_ascii_lowercase(),
            b'8' | b'9' | b'a' | b'b'
        ));
    }
    rows
}
fn packets(output: &Path, case: &str, service: &str) -> Vec<String> {
    use openpilot_cereal::log_capnp::event;
    let path = output.join(case).join(format!("{service}.bin"));
    let bytes = match fs::read(&path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return vec![],
        Err(error) => panic!("{}: {error}", path.display()),
    };
    let mut input = Cursor::new(bytes);
    let length = input.get_ref().len() as u64;
    let mut records = vec![];
    while input.position() < length {
        let message = capnp::serialize::read_message(&mut input, Default::default()).unwrap();
        let value = message.get_root::<event::Reader<'_>>().unwrap();
        assert!(value.get_valid());
        let text = match (service, value.which().unwrap()) {
            ("logMessage", event::LogMessage(v))
            | ("errorLogMessage", event::ErrorLogMessage(v)) => v.unwrap(),
            _ => panic!("wrong startup union"),
        };
        records.push(text.to_str().unwrap().to_owned());
        // logMonoTime is a live source clock, never a cross-run golden value.
    }
    records
}
#[test]
fn unchanged_original_and_native_startup_observations_agree_without_python_execution() {
    let proof = PathBuf::from(
        std::env::var_os("LOGMESSAGED_STARTUP_PROOF")
            .expect("mandatory original/native CI proof missing"),
    );
    let original = proof.join("original");
    let native = proof.join("native");
    let old_bytes = fs::read_to_string(original.join("report.json")).unwrap();
    let new_bytes = fs::read_to_string(native.join("report.json")).unwrap();
    let old: Value = serde_json::from_str(&old_bytes).unwrap();
    let new: Value = serde_json::from_str(&new_bytes).unwrap();
    for key in ["passed", "cases", "fixture_sha256"] {
        assert_eq!(old[key], new[key], "{key}");
    }
    assert_eq!(old["passed"], true);
    assert_eq!(new["compiler"], serde_json::json!([]));
    assert_eq!(
        new["initialization_gate"],
        "native source publisher reset handshake"
    );
    let command = old["compiler"].as_array().unwrap();
    assert_eq!(command.len(), 10);
    assert_eq!(
        &command[..6],
        &["cc", "-shared", "-fPIC", "-Wall", "-Wextra", "-Werror"].map(Value::from)
    );
    let expected_order = ["legacy-source", "ready-source", "ready-native"].map(String::from);
    assert_eq!(case_order(&old_bytes), expected_order);
    assert_eq!(case_order(&new_bytes), expected_order);
    assert_eq!(
        fs::read(proof.join("native.stdout")).unwrap(),
        format!("{new_bytes}\n").as_bytes()
    );
    for case in ["legacy-source", "ready-source", "ready-native"] {
        let source = json(original.join(case).join("observed.json"));
        let observed = json(native.join(case).join("observed.json"));
        assert_eq!(source, observed, "{case}");
        for service in ["logMessage", "errorLogMessage"] {
            let expected: Vec<String> = serde_json::from_value(source[service].clone()).unwrap();
            assert_eq!(
                packets(&original, case, service),
                expected,
                "original {case}/{service}"
            );
            assert_eq!(
                packets(&native, case, service),
                expected,
                "native {case}/{service}"
            );
        }
        assert_eq!(
            disk(&original, case),
            disk(&native, case),
            "{case}; only generated top-level id differs"
        );
        let worker = json(native.join(case).join("worker.stdout"));
        let capacities = worker["queue_file_bytes"].as_object().unwrap();
        assert_eq!(capacities.len(), 2);
        assert!(capacities
            .values()
            .all(|n| n.as_u64().unwrap() > 10 * 1024 * 1024));
    }
    let artifacts: BTreeMap<String, String> = fs::read_to_string(proof.join("artifacts.sha256"))
        .unwrap()
        .lines()
        .map(|line| {
            let (digest, name) = line.split_once("  ").expect("artifact SHA256 record");
            (name.into(), digest.into())
        })
        .collect();
    assert_eq!(artifacts.len(), 3); // checker, production collector and allowed native Git helper
    for (name, expected) in &artifacts {
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(name).unwrap())),
            *expected,
            "{name}"
        );
    }
    let opening = regex::Regex::new(r"\bexecve(?:at)?\(").unwrap();
    let executable = regex::Regex::new(r#"\bexecve\(("(?:[^"\\]|\\.)*")"#).unwrap();
    let mut executions = vec![];
    for line in fs::read_to_string(proof.join("native.exec"))
        .unwrap()
        .lines()
    {
        if !opening.is_match(line) {
            continue;
        }
        let path = executable
            .captures(line)
            .unwrap_or_else(|| panic!("opaque/unaccounted execve/execveat attempt: {line}"));
        let path: String = serde_json::from_str(&path[1]).unwrap();
        assert!(
            artifacts.contains_key(&path),
            "unsealed executable attempt: {line}"
        );
        let filename = Path::new(&path)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_ascii_lowercase();
        assert!(
            !filename.contains("python") && !filename.contains("pypy"),
            "{line}"
        );
        executions.push(path);
    }
    assert!(!executions.is_empty());
    let summary = serde_json::json!({"passed":true,"cases":3,"exec_attempts":executions.len(),"python_exec_attempts":0,"artifacts":artifacts,
        "compiler_metadata_transition":"original C shim recipe retained in original/report.json; native reset handshake executes no compiler"});
    fs::write(
        proof.join("comparison.json"),
        serde_json::to_vec_pretty(&summary).unwrap(),
    )
    .unwrap();
}
