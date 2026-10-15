use openpilot_dev_tools::ci_policy::parse;
use std::{fs, path::PathBuf};

#[test]
fn message_state_gate_builds_native_checker_and_preserves_service_check_and_evidence() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let workflow =
        parse(&fs::read_to_string(root.join(".github/workflows/rust.yml")).unwrap()).unwrap();
    let mut matches = 0;
    for job in workflow["jobs"].as_object().unwrap().values() {
        let Some(steps) = job["steps"].as_array() else {
            continue;
        };
        for step in steps {
            if step["name"] != "Compare message state and service catalog with original source" {
                continue;
            }
            matches += 1;
            let run = step["run"].as_str().unwrap();
            let services = run.find("python tools/check_services.py").unwrap();
            let guard = run.find("--growth-mib 512").unwrap();
            let build = run
                .find("cargo build -p openpilot-dev-tools --bin check-message-state --locked")
                .unwrap();
            let check = run.find("target/debug/check-message-state --binary target/debug/examples/state_probe --output \"$RUNNER_TEMP/message-state\"").unwrap();
            assert!(services < guard && guard < build && build < check);
            assert!(!run.contains("python tools/check_message_state.py"));
            assert!(steps.iter().any(|step| {
                step["uses"] == "actions/upload-artifact@v6"
                    && step["with"]["path"]
                        .as_str()
                        .is_some_and(|paths| paths.contains("${{ runner.temp }}/message-state/"))
            }));
        }
    }
    assert_eq!(matches, 1);
}
