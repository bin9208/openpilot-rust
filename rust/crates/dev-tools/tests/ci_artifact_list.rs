use openpilot_dev_tools::ci_artifact_list::select;
use serde_json::Value;
use std::{fs, process::Command};

#[test]
fn arm_workflow_uses_native_list_and_retains_execution_and_evidence() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let workflow = openpilot_dev_tools::ci_policy::parse(
        &fs::read_to_string(root.join(".github/workflows/rust.yml")).unwrap(),
    )
    .unwrap();
    let steps = workflow["jobs"]["radar-arm"]["steps"].as_array().unwrap();
    let run = steps
        .iter()
        .find(|step| step["name"] == "Build and execute generic ARM RadarCAN tests and artifacts")
        .unwrap()["run"]
        .as_str()
        .unwrap();
    let guard = run.find("--growth-mib 512").unwrap();
    let build = run
        .find("cargo build -p openpilot-dev-tools --bin list-cargo-tests --locked")
        .unwrap();
    let list = run.find("target/debug/list-cargo-tests \"$RUNNER_TEMP/radar-arm/tests-build.jsonl\" > \"$RUNNER_TEMP/radar-arm/tests.txt\"").unwrap();
    let nonempty = run
        .find("test -s \"$RUNNER_TEMP/radar-arm/tests.txt\"")
        .unwrap();
    let execute = run.find("\"$radar_test\" --test-threads=1").unwrap();
    assert!(guard < build && build < list && list < nonempty && nonempty < execute);
    assert!(!run.contains("python -c"));
    assert!(run.contains("done < \"$RUNNER_TEMP/radar-arm/tests.txt\""));
    assert!(run.contains("sha256sum \"$radar_test\""));
    assert!(steps
        .iter()
        .any(|step| step["uses"] == "actions/upload-artifact@v6"
            && step["if"] == "always()"
            && step["with"]["path"]
                .as_str()
                .is_some_and(|path| path.contains("${{ runner.temp }}/radar-arm/"))));
}

fn bytes(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn independently_captured_original_eager_jsonl_selection_and_stdout_bytes() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/ci-artifact-list/original.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let input = bytes(case["input_hex"].as_str().unwrap());
        let expected = bytes(case["stdout_hex"].as_str().unwrap());
        let result = select(&input);
        assert_eq!(
            result.is_ok(),
            case["exit"] == 0,
            "{}: {result:?}",
            case["name"]
        );
        if let Ok(actual) = result {
            assert_eq!(actual, expected, "{}", case["name"]);
        }
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("Cargo tests 한글.jsonl");
        fs::write(&path, &input).unwrap();
        let cli = Command::new(env!("CARGO_BIN_EXE_list-cargo-tests"))
            .arg(&path)
            .env("PATH", "")
            .output()
            .unwrap();
        assert_eq!(
            cli.status.success(),
            case["exit"] == 0,
            "{}: {cli:?}",
            case["name"]
        );
        assert_eq!(cli.stdout, expected, "{}", case["name"]);
    }
}
