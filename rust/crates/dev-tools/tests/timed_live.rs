//! Original live source arms over real Msgq/ZMQ, isolated worker environments.
use openpilot_dev_tools::{
    timed_check::Options,
    timed_live::{invoke, sources},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
#[path = "support/timed_source_pins.rs"]
mod source_pins;

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.."))
}
fn original(bytes: &[u8], sha: &str) -> Value {
    assert_eq!(format!("{:x}", Sha256::digest(bytes)), sha);
    serde_json::from_slice(bytes).unwrap()
}
fn run(case: Value, expected: Value) -> (Value, tempfile::TempDir) {
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let directory = tempfile::tempdir().unwrap();
    let options = Options {
        root: root().into(),
        binary: "/unused-native-candidate".into(),
        output: directory.path().join("artifacts"),
        reference: env!("CARGO_BIN_EXE_timed-source-reference").into(),
        command: env!("CARGO_BIN_EXE_timed-command-fixture").into(),
        worker: vec![],
    };
    let value = invoke(
        &options,
        Path::new(env!("CARGO_BIN_EXE_timed-runtime-worker")),
        &case,
    )
    .unwrap_or_else(|error| panic!("{case}: {error}"));
    assert_eq!(value["passed"], true, "{case}: {value}");
    assert_eq!(value["implementation"], "python"); // historical report label
    assert_eq!(value["exit_code"], expected["exit_code"], "{case}: {value}");
    if case["kind"] == "shutdown" && case["mode"] == "gps" {
        // The original capture retains its actual GPS epoch. Validate the new
        // real-time epoch independently; never replay that historical number.
        let commands = value["commands"].as_array().unwrap();
        assert_eq!(commands.len(), 1);
        let command = commands[0].as_array().unwrap();
        assert_eq!(command.len(), 3);
        assert_eq!(command[0], "date");
        assert_eq!(command[1], "-s");
        let epoch = command[2]
            .as_str()
            .unwrap()
            .strip_prefix('@')
            .unwrap()
            .parse::<u64>()
            .unwrap();
        let after = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(epoch >= before + 29 && epoch <= after + 31, "{value}");
    } else {
        assert_eq!(value["commands"], expected["commands"], "{case}: {value}");
    }
    let receipt: Value =
        serde_json::from_slice(&fs::read(options.output.join("ownership.json")).unwrap()).unwrap();
    assert_eq!(receipt["source_implementation"], "native source reference");
    assert_eq!(
        receipt["child_group"], receipt["worker_pid"],
        "daemon escaped owned worker group"
    );
    for field in ["shared_memory", "log_endpoint"] {
        assert!(
            !Path::new(receipt[field].as_str().unwrap()).exists(),
            "owned resource survived: {receipt}"
        );
    }
    (value, directory)
}
#[test]
fn original_both_gps_topics_real_logs_and_ten_second_sleep() {
    let data = original(
        include_bytes!("fixtures/timed-tools/daemon-original.json"),
        "d0496621763f0a057eb9e31cda5bb987637aed4c57225b0f6b9cd70852ad593c",
    );
    let expected = data["cases"].as_array().unwrap();
    assert_eq!(expected.len(), 2);
    std::thread::scope(|scope| {
        let handles = expected
            .iter()
            .enumerate()
            .map(|(index, expected)| {
                scope.spawn(move || {
                    let (actual, directory) = run(
                        json!({"kind":"daemon","external":index==1,"implementation":"python"}),
                        expected.clone(),
                    );
                    assert_eq!(actual["service"], expected["service"]);
                    let packets = actual["packets"].as_array().unwrap();
                    assert_eq!(packets.len(), 5);
                    for (packet, want) in
                        packets.iter().zip(expected["packets"].as_array().unwrap())
                    {
                        assert_eq!(packet["valid"], want["valid"]);
                        assert_eq!(packet["wall"], want["wall"]);
                    }
                    let elapsed = actual["sleep_elapsed"].as_f64().unwrap();
                    assert!(elapsed > 9.8 && elapsed < 12., "{actual}");
                    let records = actual["records"].as_array().unwrap();
                    assert_eq!(records.len(), 2);
                    for (record, want) in
                        records.iter().zip(expected["records"].as_array().unwrap())
                    {
                        for key in ["msg", "levelnum", "name"] {
                            assert_eq!(record[key], want[key]);
                        }
                        assert!(!record["pathname"].as_str().unwrap().is_empty());
                        assert!(record["lineno"].as_u64().unwrap() > 0);
                        assert!(!record["funcName"].as_str().unwrap().is_empty());
                    }
                    let output = directory.path().join("artifacts");
                    // Original transport record order survives json.loads and
                    // json.dumps; retain this report contract independently of
                    // the honest native-reference filename/PID metadata.
                    let raw_report = fs::read_to_string(output.join("result.json")).unwrap();
                    assert!(
                        raw_report.contains("\"records\": [\n    {\n      \"msg\": "),
                        "source record report key order differs"
                    );
                    for index in 0..5 {
                        assert!(!fs::read(output.join(format!("clocks-{index}.capnp")))
                            .unwrap()
                            .is_empty());
                    }
                    for index in 1..5 {
                        assert!(!fs::read(output.join(format!("gps-{index}.capnp")))
                            .unwrap()
                            .is_empty());
                    }
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle.join().unwrap();
        }
    });
}
#[test]
fn original_eight_extreme_date_process_exit_and_continuation_boundaries() {
    let data = original(
        include_bytes!("fixtures/timed-tools/date-daemon-original.json"),
        "356cbf4e497716aa4a8c219efd33c175de08b2bfc7452dabcb858f9483f5ae5d",
    );
    let cases = data["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 8);
    for batch in cases.chunks(4) {
        std::thread::scope(|scope| {
            let handles=batch.iter().map(|expected|scope.spawn(move||{
            let (actual,directory)=run(json!({"kind":"date-daemon","zone":expected["zone"],"milliseconds":expected["unix_timestamp_millis"],"implementation":"python"}),expected.clone());
            for key in ["source_error","zone","unix_timestamp_millis","clock_packet_sizes"]{assert_eq!(actual[key],expected[key],"{actual}");}
            assert!(!fs::read(directory.path().join("artifacts/gps.capnp")).unwrap().is_empty());
        })).collect::<Vec<_>>();
            for handle in handles {
                handle.join().unwrap();
            }
        });
    }
}
#[test]
fn original_real_sleep_phase_and_progressing_proxy_shutdown() {
    let data = original(
        include_bytes!("fixtures/timed-tools/shutdown-original.json"),
        "a9092c103e6ad0e8a4fca2349f6349756747f67d99ca093788cd599a300ea09d",
    );
    let cases = data["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2);
    std::thread::scope(|scope| {
        let handles=cases.iter().map(|expected|scope.spawn(move||{
            let (actual,directory)=run(json!({"kind":"shutdown","mode":expected["mode"],"signal":"SIGINT","implementation":"python"}),expected.clone());
            assert!(actual["elapsed"].as_f64().unwrap()<2.);assert_eq!(actual["peer_closed"],true);
            if expected["mode"]=="gps" {
                assert_eq!(actual["source_phase"]["wchan"],"hrtimer_nanosleep");
                assert!(directory.path().join("artifacts/sleep-readiness.json").is_file());
            }else{
                assert!(actual["source_phase"].is_null());let requests=actual["requests"].as_array().unwrap();assert_eq!(requests.len(),1);
                assert_eq!(requests[0]["path"],expected["requests"][0]["path"]);
                assert!(requests[0]["chunks"].as_u64().unwrap()>40);
            }
        })).collect::<Vec<_>>();
        for handle in handles {
            handle.join().unwrap();
        }
    });
}
#[test]
fn all_live_original_source_pins_remain_required() {
    let data: Value =
        serde_json::from_slice(include_bytes!("fixtures/timed-tools/live-manifest.json")).unwrap();
    source_pins::verify(root(), data["sources"].as_object().unwrap(), sources);
}

#[test]
fn original_worker_integer_admission_and_post_clock_schema_failure_order() {
    use std::process::Command;
    let original = original(
        include_bytes!("fixtures/timed-tools/live-worker-original.json"),
        "98e310cedc9f77c736cbec3915527b060ba3c7fed9e1ed7fe8df9046d0aa6ff9",
    );
    let rows = original["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 7);
    for row in rows {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("output");
        let actual = Command::new(env!("CARGO_BIN_EXE_check-timed-date-daemon"))
            .args(["--binary", "/unused-native-candidate", "--output"])
            .arg(&output)
            .args([
                "--worker",
                "UTC",
                row["milliseconds"].as_str().unwrap(),
                "python",
            ])
            .env("OPENPILOT_RUST_ROOT", root())
            .output()
            .unwrap();
        assert_eq!(
            actual.status.code().map(i64::from),
            row["exit_code"].as_i64(),
            "{row}: {}",
            String::from_utf8_lossy(&actual.stderr)
        );
        assert_eq!(actual.stdout, row["stdout"].as_str().unwrap().as_bytes());
        assert_eq!(
            output.exists(),
            row["output_exists"].as_bool().unwrap(),
            "{row}"
        );
        let mut files = if output.exists() {
            fs::read_dir(&output)
                .unwrap()
                .map(|item| item.unwrap().file_name().into_string().unwrap())
                .filter(|name| name != "ownership.json")
                .collect::<Vec<_>>()
        } else {
            vec![]
        };
        files.sort();
        assert_eq!(serde_json::json!(files), row["files"], "{row}");
        if let Some(error) = row["error_class"].as_str() {
            assert!(
                String::from_utf8_lossy(&actual.stderr).contains(error),
                "{row}: {}",
                String::from_utf8_lossy(&actual.stderr)
            );
        }
        if let Some(expected) = row.get("result") {
            let observed: Value =
                serde_json::from_slice(&fs::read(output.join("result.json")).unwrap()).unwrap();
            assert_eq!(&observed, expected, "{row}");
        }
    }
}
