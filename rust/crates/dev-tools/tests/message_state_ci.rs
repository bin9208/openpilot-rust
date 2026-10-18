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
            let service_guard = run.find("--growth-mib 512").unwrap();
            let service_build = run.find("cargo build -p openpilot-dev-tools --features services-catalog-native --bin check-services --locked").unwrap();
            let services = run.find("target/debug/check-services").unwrap();
            let guard = services + run[services..].find("--growth-mib 512").unwrap();
            let build = run
                .find("cargo build -p openpilot-dev-tools --bin check-message-state --locked")
                .unwrap();
            let check = run.find("target/debug/check-message-state --binary target/debug/examples/state_probe --output \"$RUNNER_TEMP/message-state\"").unwrap();
            assert!(services < guard && guard < build && build < check);
            assert!(service_guard < service_build && service_build < services);
            assert!(!run.contains("python tools/check_services.py"));
            assert!(run.contains("cargo test -p openpilot-dev-tools --features services-catalog-native --test services_catalog_check --locked"));
            assert!(run.contains("cargo clippy -p openpilot-dev-tools --features services-catalog-native --bin check-services --test services_catalog_check --locked -- -D warnings"));
            assert_ne!(step["continue-on-error"], true);
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
