//! Independent pre-port source captures; candidate timed code is not an oracle.
use openpilot_dev_tools::timed_reference::{run_case, sources};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

#[path = "support/timed_source_pins.rs"]
mod source_pins;

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.."))
}
fn corpus(bytes: &[u8], digest: &str) -> Value {
    assert_eq!(format!("{:x}", Sha256::digest(bytes)), digest);
    serde_json::from_slice(bytes).unwrap()
}
fn original() -> Value {
    corpus(
        include_bytes!("fixtures/timed-tools/reference-original.json"),
        "c9a30ee09a868cfa96a54e6d4a96e3da26dd1c61b1d23390f0e59e075d7e2bf7",
    )
}

#[test]
fn original_reference_and_complete_live_source_pin_set() {
    let data = original();
    assert_eq!(data["cases"].as_array().unwrap().len(), 54);
    assert_eq!(data["summary"]["source_native_scenarios"], 54);
    assert_eq!(data["provenance"]["implementation"], "CPython");
    source_pins::verify(
        root(),
        data["provenance"]["sources"].as_object().unwrap(),
        sources,
    );
}

#[test]
fn all_54_original_policy_filesystem_command_and_loop_scenarios() {
    let data = original();
    for case in data["cases"].as_array().unwrap() {
        let temporary = tempfile::tempdir().unwrap();
        let mut request = case.clone();
        for key in ["values", "errors", "state"] {
            request.as_object_mut().unwrap().remove(key);
        }
        let observed = run_case(
            root(),
            &request,
            temporary.path(),
            Path::new(env!("CARGO_BIN_EXE_timed-source-reference")),
            Path::new(env!("CARGO_BIN_EXE_timed-command-fixture")),
        )
        .unwrap_or_else(|error| panic!("{}: {error}", case["name"]));
        for key in ["values", "errors", "state"] {
            assert_eq!(observed[key], case[key], "{} {key}", case["name"]);
        }
    }
}

#[test]
fn all_360_original_local_datetime_fold_and_error_boundaries() {
    let data = corpus(
        include_bytes!("fixtures/timed-tools/dates-original.json"),
        "bea30075403cc174994435c83e07997865053a929997d5bf72506ba062c94774",
    );
    assert_eq!(data["cases"].as_array().unwrap().len(), 360);
    for case in data["cases"].as_array().unwrap() {
        let request = json!({
            "timezone":case["zone"], "loop":true,
            "config":{"params_root":"<root>","paths":{"zoneinfo":"<root>/zoneinfo", "localtime":"<root>/etc/localtime", "systemd":"<root>/systemd"},"endpoint":"http://127.0.0.1:1/unused", "wall":1790000000000000000u64, "monotonic":0,"actions":[case["action"]]},
            "initial":{"params_hex":{"TimezoneSource":"617070"},"command_fail":null,"sudo_exists":true,"localtime":null,"systemd_mtime_ns":null}
        });
        let temporary = tempfile::tempdir().unwrap();
        let observed = run_case(
            root(),
            &request,
            temporary.path(),
            Path::new(env!("CARGO_BIN_EXE_timed-source-reference")),
            Path::new(env!("CARGO_BIN_EXE_timed-command-fixture")),
        )
        .unwrap_or_else(|error| panic!("{} {}: {error}", case["zone"], case["milliseconds"]));
        assert_eq!(
            observed["errors"][0]["message"], case["expected"]["error"],
            "{case}"
        );
        assert_eq!(
            observed["errors"][0]["type"], case["expected"]["error_type"],
            "{case}"
        );
        assert_eq!(
            observed["values"]["sleeps"], case["expected"]["sleeps"],
            "{case}"
        );
        assert_eq!(observed["state"], case["expected"]["state"], "{case}");
    }
}

#[test]
fn original_log_host_uses_real_kernel_identity_when_hostname_environment_is_missing_or_spoofed() {
    use openpilot_dev_tools::{
        qa_process::{CommandSpec, NativeRunner, Runner},
        timed_fixtures::Fixture,
    };
    use std::time::Duration;
    let bytes = include_bytes!("fixtures/timed-tools/hostname-original.json");
    let original = corpus(
        bytes,
        "0582ec93f54e2453e5d4b778bdb941a880a5e2e509436e6c0d7fbb11a123c029",
    );
    let cases = original["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2);
    // Independent kernel filesystem observation, not the implementation's uname
    // API or the historical capture machine's hostname.
    let actual_hostname = fs::read_to_string("/proc/sys/kernel/hostname")
        .unwrap()
        .trim_end_matches('\n')
        .to_owned();
    for case in cases {
        assert_eq!(case["record_host"], case["actual_hostname"]);
        let directory = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::new(
            directory.path(),
            "UTC",
            Path::new(env!("CARGO_BIN_EXE_timed-command-fixture")),
        )
        .unwrap();
        fixture.config["actions"] = json!([{"kind":"set_time","epoch":1790000030}]);
        let mut spec = CommandSpec::new_os(
            vec![env!("CARGO_BIN_EXE_timed-source-reference").into()],
            Duration::from_secs(10),
        );
        fixture.configure(&mut spec);
        if let Some(spoof) = case["hostname_environment"].as_str() {
            spec.os_env
                .as_mut()
                .unwrap()
                .insert("HOSTNAME".into(), spoof.into());
        } else {
            spec.env_remove.push("HOSTNAME".into());
        }
        spec.input =
            Some(format!("{}\n", json!({"config":fixture.config,"logging":true})).into_bytes());
        let captured = NativeRunner.run(&spec).unwrap();
        assert_eq!(
            captured.code,
            0,
            "{}",
            String::from_utf8_lossy(&captured.stderr)
        );
        let report: Value = serde_json::from_slice(&captured.stdout).unwrap();
        let records = report["records"].as_array().unwrap();
        assert_eq!(records.len(), 1, "{report}");
        assert_eq!(records[0]["host"], actual_hostname, "{case}: {report}");
        assert_eq!(records[0]["msg"], case["msg"]);
        assert_eq!(records[0]["levelnum"], case["levelnum"]);
    }
}
