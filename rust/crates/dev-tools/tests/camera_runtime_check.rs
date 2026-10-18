//! Independent original runtime verifier and partial-artifact contracts.
#![cfg(feature = "camera-sensor-native")]
use flate2::read::GzDecoder;
use openpilot_dev_tools::{
    camera_runtime_check::{verify, Options},
    camera_state_check::{Decoded, NativeSchemaDecoder, SchemaDecoder},
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    ffi::OsStr,
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

// Every distinct byte sequence still reaches the actual external schema oracle.
// Repeated negative controls reuse that immutable result, retaining all cases.
struct CachedDecoder {
    native: NativeSchemaDecoder,
    values: HashMap<Vec<u8>, (openpilot_log_format::JsonValue, Option<String>)>,
}
impl SchemaDecoder for CachedDecoder {
    fn load(&mut self, path: &Path) -> Result<()> {
        self.values.clear();
        self.native.load(path)
    }
    fn decode(&mut self, wire: &[u8]) -> Result<Decoded> {
        if let Some((value, serialization_error)) = self.values.get(wire) {
            return Ok(Decoded {
                value: value.clone(),
                serialization_error: serialization_error.clone(),
            });
        }
        let decoded = self.native.decode(wire)?;
        self.values.insert(
            wire.to_vec(),
            (decoded.value.clone(), decoded.serialization_error.clone()),
        );
        Ok(decoded)
    }
}

fn directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/camera-runtime-check")
}
fn fixture(name: &str) -> Value {
    serde_json::from_slice(&fs::read(directory().join(name)).unwrap()).unwrap()
}
fn compressed(name: &str) -> Value {
    let file = fs::File::open(directory().join(name)).unwrap();
    serde_json::from_reader(GzDecoder::new(file)).unwrap()
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
struct OraclePeer<'a> {
    expected: &'a Value,
    root: &'a Path,
    calls: usize,
}
impl Runner for OraclePeer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let call = &self.expected["calls"][self.calls];
        self.calls += 1;
        assert_eq!(spec.timeout, Duration::from_secs(10));
        assert_eq!(spec.stdout_limit, usize::MAX);
        assert_eq!(spec.stderr_limit, usize::MAX);
        assert!(!spec.inherit_output && !spec.inherit_stdin && !spec.merge_output);
        assert_eq!(
            spec.input.as_deref(),
            Some(call["input"].as_str().unwrap().as_bytes())
        );
        let actual = spec
            .effective_argv()
            .iter()
            .map(|v| {
                v.to_str()
                    .unwrap()
                    .replace(self.root.to_str().unwrap(), "$ROOT")
            })
            .collect::<Vec<_>>();
        assert_eq!(serde_json::to_value(actual).unwrap(), call["argv"]);
        let env = spec.effective_env();
        assert_eq!(
            env.get(OsStr::new("LD_LIBRARY_PATH")).unwrap(),
            OsStr::new(call["ld_library_path"].as_str().unwrap())
        );
        assert!(spec.env_remove.is_empty());
        let Some(stdout) = call["stdout"].as_str() else {
            return Err("controlled oracle launch failure".into());
        };
        Ok(Captured {
            code: call["exit"].as_i64().unwrap() as i32,
            stdout: stdout.as_bytes().to_vec(),
            stderr: call["stderr"].as_str().unwrap().as_bytes().to_vec(),
        })
    }
}
fn original_verifier_cases(baselines: bool, coercions: bool) {
    verifier_fixture_cases(
        baselines,
        coercions.then_some("verify-coercions-original.json"),
        if baselines || coercions { 8 } else { 36 },
    );
}
fn verifier_fixture_cases(baselines: bool, additional: Option<&str>, count: usize) {
    let original = fixture(additional.unwrap_or("verify-original.json"));
    let program = std::env::var_os("OPENPILOT_CAPNP").unwrap_or_else(|| "capnp".into());
    let mut decoder = CachedDecoder {
        native: NativeSchemaDecoder::new(program.into()),
        values: HashMap::new(),
    };
    decoder.load(&directory().join("schema/log.capnp")).unwrap();
    let mut visited = 0;
    for case in original["cases"].as_array().unwrap() {
        let name = case["case"].as_str().unwrap();
        let mode = case["mode"].as_str().unwrap();
        if additional.is_none() && (mode == "baseline") != baselines {
            continue;
        }
        visited += 1;
        let base = compressed(&format!("{name}-original.json.gz"));
        let changes = compressed(&format!("inputs/{name}--{mode}.json.gz"));
        let mut input_files = base["files"].as_object().unwrap().clone();
        for key in changes["remove"].as_array().unwrap() {
            input_files.remove(key.as_str().unwrap());
        }
        for (key, value) in changes["files"].as_object().unwrap() {
            input_files.insert(key.clone(), value.clone());
        }
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let output = root.join("out");
        fs::create_dir(&output).unwrap();
        for (filename, value) in &input_files {
            fs::write(output.join(filename), value.as_str().unwrap()).unwrap();
        }
        for filename in ["binary", "fixture", "oracle"] {
            fs::write(root.join(filename), filename).unwrap();
        }
        if mode == "binary-missing" {
            fs::remove_file(root.join("binary")).unwrap();
        }
        let options = Options {
            binary: root.join("binary"),
            fixture: root.join("fixture"),
            oracle: root.join("oracle"),
            output: output.clone(),
            case: name.to_owned(),
            qemu: mode.starts_with("qemu").then(|| root.join("qemu")),
            sysroot: (mode != "qemu-missing-sysroot").then(|| root.join("sysroot")),
        };
        let mut oracle = OraclePeer {
            expected: case,
            root,
            calls: 0,
        };
        let mut printed = vec![];
        let result = verify(&options, &mut oracle, &mut decoder, &mut printed);
        assert_eq!(
            result.is_ok(),
            case["success"].as_bool().unwrap(),
            "{name}/{mode}: {result:?}"
        );
        if let Some(expected) = case["error"]["message"].as_str() {
            assert_eq!(
                result.as_ref().err().unwrap().to_string(),
                expected,
                "{name}/{mode} exception message"
            );
        }
        assert_eq!(
            oracle.calls,
            case["calls"].as_array().unwrap().len(),
            "{name}/{mode} process boundary"
        );
        assert_eq!(
            String::from_utf8(printed)
                .unwrap()
                .replace(root.to_str().unwrap(), "$ROOT"),
            case["stdout"],
            "{name}/{mode} printed output"
        );
        for (filename, expected) in &input_files {
            assert_eq!(
                fs::read_to_string(output.join(filename)).unwrap(),
                expected.as_str().unwrap(),
                "{name}/{mode} retained input {filename}"
            );
        }
        let mut generated = Map::new();
        for entry in fs::read_dir(&output).unwrap() {
            let path = entry.unwrap().path();
            let filename = path.file_name().unwrap().to_str().unwrap();
            if input_files.contains_key(filename) {
                continue;
            }
            generated.insert(
                filename.to_owned(),
                fs::read_to_string(path)
                    .unwrap()
                    .replace(root.to_str().unwrap(), "$ROOT")
                    .into(),
            );
        }
        assert_eq!(
            Value::Object(generated),
            case["files"],
            "{name}/{mode} exact partial artifacts"
        );
    }
    assert_eq!(visited, count);
}
#[test]
fn all_eight_original_runtime_cases_verify_actual_wires_and_exact_artifacts() {
    original_verifier_cases(true, false);
}
#[test]
fn original_verifier_failures_preserve_all_partial_artifacts_and_process_order() {
    original_verifier_cases(false, false);
}
#[test]
fn original_round_comparisons_and_unrelated_operation_types_preserve_failure_order() {
    original_verifier_cases(false, true);
}
#[test]
fn original_complete_corpus_and_source_schema_digests_are_preserved() {
    let manifest = fixture("verify-original.json");
    assert_eq!(manifest["cases"].as_array().unwrap().len(), 44);
    for (name, expected) in manifest["baselines"].as_object().unwrap() {
        let packed = fs::read(directory().join(format!("{name}-original.json.gz"))).unwrap();
        assert_eq!(digest(&packed), expected["gzip_sha256"]);
        let mut raw = vec![];
        GzDecoder::new(packed.as_slice())
            .read_to_end(&mut raw)
            .unwrap();
        assert_eq!(digest(&raw), expected["json_sha256"]);
        assert_eq!(raw.len() as u64, expected["json_bytes"].as_u64().unwrap());
    }
    let pins = fixture("schema-original.json");
    let entries = pins["files"].as_object().unwrap();
    assert_eq!(
        entries.keys().map(String::as_str).collect::<Vec<_>>(),
        [
            "car.capnp",
            "custom.capnp",
            "deprecated.capnp",
            "include/c++.capnp",
            "log.capnp",
        ]
    );
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for (filename, pin) in entries {
        let schema = fs::read(directory().join("schema").join(filename)).unwrap();
        assert_eq!(digest(&schema), pin["sha256"]);
        assert_eq!(
            schema,
            fs::read(source_root.join(pin["source"].as_str().unwrap())).unwrap()
        );
    }
}

#[test]
fn original_log_prefix_failures_preserve_python_type_and_phase() {
    verifier_fixture_cases(false, Some("verify-log-prefix-original.json"), 4);
}
#[test]
fn original_log_membership_and_short_circuit_preserve_arbitrary_json_values() {
    verifier_fixture_cases(false, Some("verify-log-membership-original.json"), 4);
}
