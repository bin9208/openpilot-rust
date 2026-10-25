//! Captured unchanged source generators, not native camera candidate outputs.
use openpilot_dev_tools::camera_lifecycle_generate::{extract, generate, Kind};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn kind(value: &str) -> (Kind, bool) {
    match value {
        "ae" => (Kind::Ae, false),
        "misc" => (Kind::Misc, false),
        "kernel" => (Kind::Kernel, false),
        "sensor" => (Kind::SensorLifecycle, false),
        "sensor-fixture" => (Kind::SensorLifecycle, true),
        "isp" => (Kind::IspLifecycle, false),
        "isp-fixture" => (Kind::IspLifecycle, true),
        "camera" => (Kind::CameraLifecycle, false),
        "camera-fixture" => (Kind::CameraLifecycle, true),
        "state" => (Kind::State, false),
        "stress" => (Kind::Stress, false),
        "runtime" => (Kind::RuntimeFixture, false),
        _ => panic!("unknown independently captured kind {value}"),
    }
}
fn binary(value: &str) -> &'static str {
    match value {
        "ae" => env!("CARGO_BIN_EXE_generate-camerad-ae-reference"),
        "kernel" => env!("CARGO_BIN_EXE_generate-camerad-kernel-reference"),
        "sensor" => env!("CARGO_BIN_EXE_generate-camerad-sensor-lifecycle-reference"),
        "isp" => env!("CARGO_BIN_EXE_generate-camerad-isp-lifecycle-reference"),
        "camera" => env!("CARGO_BIN_EXE_generate-camerad-camera-lifecycle-reference"),
        "state" => env!("CARGO_BIN_EXE_generate-camerad-state-reference"),
        "stress" => env!("CARGO_BIN_EXE_generate-camerad-stress-reference"),
        "runtime" => env!("CARGO_BIN_EXE_generate-camerad-runtime-fixture"),
        _ => panic!("unknown CLI {value}"),
    }
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn captured() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "fixtures/camera-lifecycle-generators/original.json"
    ))
    .unwrap()
}
fn tree(path: &Path, record: &serde_json::Value, changes: &serde_json::Value) {
    for name in record["sources"].as_object().unwrap().keys() {
        let data = fs::read(root().join(name));
        if let Ok(data) = data {
            let target = path.join(name);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, data).unwrap();
        } else {
            // Only retired generator scripts may be absent; they are not runtime inputs.
            assert!(
                name.starts_with("rust/tools/") && name.ends_with(".py"),
                "missing live pin {name}"
            );
        }
    }
    for change in changes.as_array().unwrap() {
        if let Some(mode) = change["all_newlines"].as_str() {
            for name in record["sources"].as_object().unwrap().keys() {
                let target = path.join(name);
                if !target.exists() {
                    continue;
                }
                let bytes = fs::read(target.clone()).unwrap();
                let text = String::from_utf8(bytes).unwrap();
                fs::write(
                    target,
                    text.replace('\n', if mode == "crlf" { "\r\n" } else { "\r" }),
                )
                .unwrap();
            }
            continue;
        }
        let target = path.join(change["path"].as_str().unwrap());
        if change["remove"] == true {
            fs::remove_file(target).unwrap();
            continue;
        }
        let mut data = fs::read(&target).unwrap();
        if change["invalid_utf8"] == true {
            data.push(255);
            fs::write(target, data).unwrap();
            continue;
        }
        let mut text = String::from_utf8(data).unwrap();
        if let Some(value) = change["set"].as_str() {
            text = value.into();
        }
        if let Some(value) = change["append"].as_str() {
            text.push_str(value);
        }
        if let Some(pair) = change["replace"].as_array() {
            text = text.replace(pair[0].as_str().unwrap(), pair[1].as_str().unwrap());
        }
        if let Some(signature) = change["after_signature"].as_str() {
            let start = text.find(signature).unwrap();
            let at = start + text[start..].find('{').unwrap() + 1;
            text.insert_str(at, "\n// independently changed current source\n");
        }
        fs::write(target, text).unwrap();
    }
}
#[test]
fn unchanged_source_and_fixture_outputs_preserve_extraction_errors_and_newlines() {
    let original = captured();
    for (name, expected) in original["sources"].as_object().unwrap() {
        match fs::read(root().join(name)) {
            Ok(bytes) => assert_eq!(sha(&bytes), expected.as_str().unwrap(), "source pin {name}"),
            Err(error) if name.ends_with(".py") && error.kind() == std::io::ErrorKind::NotFound => {
            }
            Err(error) => panic!("missing live source {name}: {error}"),
        }
    }
    for case in original["cases"].as_array().unwrap() {
        let temporary = tempfile::tempdir().unwrap();
        tree(temporary.path(), &original, &case["changes"]);
        let (kind, fixture) = kind(case["kind"].as_str().unwrap());
        let result = generate(temporary.path(), kind, fixture);
        assert_eq!(
            result.is_ok(),
            case["exit"] == 0,
            "{}/{}: {result:?}",
            case["kind"],
            case["mode"]
        );
        let output = result.unwrap_or_default();
        assert_eq!(
            output.len() as u64,
            case["stdout_bytes"].as_u64().unwrap(),
            "{}/{} length",
            case["kind"],
            case["mode"]
        );
        assert_eq!(
            sha(output.as_bytes()),
            case["stdout_sha256"],
            "{}/{} bytes",
            case["kind"],
            case["mode"]
        );
    }
}
#[test]
fn shared_extractor_keeps_original_naive_brace_count_and_error_boundary() {
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/camera-lifecycle-generators/extract.json"
    ))
    .unwrap();
    for case in cases.as_array().unwrap() {
        let result = extract(
            case["text"].as_str().unwrap(),
            case["signature"].as_str().unwrap(),
        );
        if let Some(output) = case["output"].as_str() {
            assert_eq!(result.unwrap(), output);
        } else {
            assert!(result.is_err(), "{case}");
        }
    }
}
#[test]
fn native_only_cli_retains_original_argument_and_stdout_exit_classes() {
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/camera-lifecycle-generators/cli.json"
    ))
    .unwrap();
    for case in cases.as_array().unwrap() {
        let arguments = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|arg| {
                arg.as_str()
                    .unwrap()
                    .replace("$ROOT", root().to_str().unwrap())
            })
            .collect::<Vec<_>>();
        let output = Command::new(binary(case["kind"].as_str().unwrap()))
            .args(arguments)
            .env("PATH", "/nonexistent")
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(case["exit"].as_i64().unwrap() as i32),
            "{case}: {:?}",
            output.stderr
        );
        assert_eq!(
            !output.stdout.is_empty(),
            case["stdout"].as_bool().unwrap(),
            "{case}"
        );
        assert_eq!(
            !output.stderr.is_empty(),
            case["stderr"].as_bool().unwrap(),
            "{case}"
        );
        if let Some(hash) = case["stdout_sha256"].as_str() {
            assert_eq!(sha(&output.stdout), hash, "{case} exact native-only stdout");
            assert_eq!(
                output.stdout.len() as u64,
                case["stdout_bytes"].as_u64().unwrap(),
                "{case}"
            );
        }
    }
}
