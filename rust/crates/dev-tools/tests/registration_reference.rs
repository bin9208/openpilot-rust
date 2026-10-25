//! Immutable unchanged original policy through real signed HTTP, Params and IPC.
#![cfg(feature = "registration-tools-native")]
#[path = "support/registration_original.rs"]
mod original;
use openpilot_dev_tools::registration_check::{
    self as native, reference, Case, ReferenceOptions, Suite,
};
use openpilot_log_format::{JsonValue, JsonView};
use original::{array, assert_json, field, read, root, text, worker};
use std::{collections::BTreeMap, fs, io::Cursor, path::Path, time::Duration};

fn input(row: &JsonValue) -> Case {
    let pair = array(row);
    Case {
        name: text(&pair[0]),
        input: pair[1].clone(),
    }
}
fn options(output: &Path) -> ReferenceOptions {
    ReferenceOptions {
        root: root(),
        output: output.to_owned(),
        environment: std::env::vars_os().collect(),
    }
}
fn packets(path: &Path, error: bool) -> Vec<JsonValue> {
    let raw = fs::read(path).unwrap_or_default();
    let mut cursor = Cursor::new(&raw);
    let mut rows = Vec::new();
    while (cursor.position() as usize) < raw.len() {
        let message =
            capnp::serialize::read_message(&mut cursor, capnp::message::ReaderOptions::new())
                .unwrap();
        let event = message
            .get_root::<openpilot_cereal::log_capnp::event::Reader>()
            .unwrap();
        assert!(event.get_valid());
        assert!(event.get_log_mono_time() > 0);
        let payload = match event.which().unwrap() {
            openpilot_cereal::log_capnp::event::LogMessage(t) if !error => t.unwrap(),
            openpilot_cereal::log_capnp::event::ErrorLogMessage(t) if error => t.unwrap(),
            _ => panic!("wrong actual collector topic"),
        };
        rows.push(JsonValue::parse(payload.to_str().unwrap()).unwrap());
    }
    rows
}
fn level(record: &JsonValue) -> i64 {
    match field(record, "levelnum").view() {
        JsonView::Integer(v) => v.parse().unwrap(),
        _ => panic!("level is not integer"),
    }
}
fn assert_artifacts(
    path: &Path,
    expected: &JsonValue,
    public_keys: &BTreeMap<String, String>,
    input: &JsonValue,
) {
    // Actual full records, cereal and persisted disk data must agree independently
    // of the controller's summary booleans. Removing collection would fail here.
    let records = array(&read(&path.join("records.json")));
    assert_eq!(records.len(), array(&field(expected, "records")).len());
    let received = packets(&path.join("logMessage.bin"), false);
    assert_eq!(received.len(), records.len());
    for (a, b) in received.iter().zip(&records) {
        assert_json(a, b, "complete logMessage packet");
    }
    let errors = packets(&path.join("errorLogMessage.bin"), true);
    let wanted = records
        .iter()
        .filter(|row| level(row) >= 40)
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), wanted.len());
    for (a, b) in errors.iter().zip(wanted) {
        assert_json(a, b, "complete errorLogMessage packet");
    }
    let disk = array(&read(&path.join("disk-records.json")));
    let wanted = records
        .iter()
        .filter(|row| level(row) >= 20)
        .collect::<Vec<_>>();
    assert_eq!(disk.len(), wanted.len());
    for (a, b) in disk.iter().zip(wanted) {
        assert_json(a, b, "complete persisted source disk record");
    }
    let config = read(&path.join("config.json"));
    let log_root = std::path::PathBuf::from(text(&field(&config, "log_root")));
    let mut files = fs::read_dir(log_root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .as_encoded_bytes()
                .starts_with(b"swaglog.")
        })
        .collect::<Vec<_>>();
    files.sort();
    let mut persisted = Vec::new();
    for file in files {
        for line in fs::read_to_string(file).unwrap().lines() {
            let row = JsonValue::parse(line).unwrap();
            let JsonView::Object(fields) = row.view() else {
                panic!("nonobject disk record")
            };
            let fields = fields
                .iter()
                .filter(|(key, _)| !key.iter().copied().eq("id".chars().map(u32::from)))
                .map(|(key, value)| {
                    let key = if key.iter().copied().eq("msg$s".chars().map(u32::from)) {
                        JsonValue::text("msg")
                    } else {
                        JsonValue::codepoints(key.to_vec()).unwrap()
                    };
                    format!("{}:{}", key.to_json().unwrap(), value.to_json().unwrap())
                })
                .collect::<Vec<_>>()
                .join(",");
            persisted.push(JsonValue::parse(&format!("{{{fields}}}")).unwrap());
        }
    }
    assert_eq!(
        persisted.len(),
        disk.len(),
        "actual disk files, not only an adapter summary"
    );
    for (actual, expected) in persisted.iter().zip(&disk) {
        assert_json(actual, expected, "actual source disk file");
    }
    let actual = read(&path.join("result.json"));
    assert_json(
        &field(&actual, "trace"),
        &field(&field(expected, "result"), "trace"),
        "raw source trace",
    );
    assert_json(
        &field(&actual, "params_hex"),
        &field(&field(expected, "result"), "params_hex"),
        "raw Params bytes",
    );
    let invocation = read(&path.join("invocation.json"));
    assert_json(
        &field(&invocation, "exit"),
        &JsonValue::parse("0").unwrap(),
        "acknowledged source exit",
    );
    let raw = read(&path.join("requests.json"));
    let utc = input
        .get("config")
        .and_then(|v| v.get("utc"))
        .map_or(1_700_000_000, |v| {
            v.to_json().unwrap().parse::<i64>().unwrap()
        });
    assert_json(
        &reference::normalize_requests(&raw, public_keys, utc + 3600).unwrap(),
        &field(expected, "requests"),
        "independently captured signed requests",
    );
    // Real signature/key/claim verification is mandatory, not token decoding.
    if !array(&raw).is_empty() {
        let empty = BTreeMap::new();
        assert!(reference::normalize_requests(&raw, &empty, utc + 3600).is_err());
        assert!(reference::normalize_requests(&raw, public_keys, utc + 3601).is_err());
        let mut encoded = raw.to_json().unwrap().into_bytes();
        let marker = b"register_token=";
        let start = encoded
            .windows(marker.len())
            .position(|v| v == marker)
            .unwrap()
            + marker.len();
        let end = encoded[start..]
            .iter()
            .position(|b| matches!(b, b'&' | b' '))
            .unwrap()
            + start;
        let signature = encoded[start..end]
            .iter()
            .rposition(|b| *b == b'.')
            .unwrap()
            + start
            + 1;
        encoded[signature] = if encoded[signature] == b'A' {
            b'B'
        } else {
            b'A'
        };
        let forged = JsonValue::parse(std::str::from_utf8(&encoded).unwrap()).unwrap();
        assert!(
            reference::normalize_requests(&forged, public_keys, utc + 3600).is_err(),
            "a forged token signature cannot satisfy the expected request"
        );
    }
}
fn check(roster: &str, expected: &str) {
    let fixture = read(&original::fixture("inputs-original.json"));
    let expectations = read(&original::fixture("source-baseline-original.json"));
    let rows = array(&field(&fixture, roster));
    let wanted = array(&field(&expectations, expected));
    let worker = worker();
    let temporary = tempfile::tempdir().unwrap();
    for row in rows {
        let case = input(&row);
        let expected = wanted
            .iter()
            .find(|r| text(&field(r, "case")) == case.name)
            .unwrap();
        let path = temporary.path().join(&case.name);
        let observed = reference::observe(&options(&path), &case, &worker)
            .unwrap_or_else(|e| panic!("{}: {e}", case.name));
        assert_json(&observed.normalized, expected, &case.name);
        let minimum = case
            .input
            .get("minimum_seconds")
            .map_or(0, |n| n.to_json().unwrap().parse::<u64>().unwrap());
        assert!(
            observed.elapsed >= Duration::from_secs(minimum),
            "{} elapsed too short",
            case.name
        );
        assert_artifacts(&path, expected, &observed.public_keys, &case.input);
    }
}
#[test]
fn all_131_original_policies_keep_real_params_signed_requests_logs_and_collector_shutdown() {
    check("matrix", "cases")
}
#[test]
fn each_real_timeout_is_per_io_and_progressing_response_may_exceed_fifteen_seconds() {
    check("timeouts", "cases")
}
#[test]
fn arbitrary_inputs_use_dynamic_policy_instead_of_a_builtin_case_name_lookup() {
    check("generic", "generic")
}
#[test]
fn full_generated_rosters_keep_original_order_settings_and_timeout_precedence() {
    let inputs = read(&original::fixture("inputs-original.json"));
    for (suite, key, count) in [
        (Suite::All, "matrix", 131),
        (Suite::Hardware, "matrix", 12),
        (Suite::Timeouts, "timeouts", 3),
    ] {
        let generated = native::cases(suite).unwrap();
        let mut expected = array(&field(&inputs, key));
        if matches!(suite, Suite::Hardware) {
            expected.drain(..119);
        }
        assert_eq!(generated.len(), count);
        for (actual, old) in generated.iter().zip(expected) {
            let old = input(&old);
            assert_eq!(actual.name, old.name);
            assert_json(&actual.input, &old.input, &actual.name);
        }
    }
}

#[test]
fn delayed_real_error_publisher_resets_then_refreshes_the_actual_reader_before_error_workload() {
    let inputs = read(&original::fixture("inputs-original.json"));
    let row = array(&field(&inputs, "matrix"))
        .into_iter()
        .find(|row| text(&array(row)[0]) == "clock_year_ten_thousand")
        .unwrap();
    let case = input(&row);
    let temporary = tempfile::tempdir().unwrap();
    let output = temporary.path().join("delayed");
    let mut options = options(&output);
    options
        .environment
        .insert("REGISTRATION_COLLECTOR_DELAY_MS".into(), "800".into());
    let observed = reference::observe(&options, &case, &worker()).unwrap();
    let expected = array(&field(
        &read(&original::fixture("source-baseline-original.json")),
        "cases",
    ))
    .into_iter()
    .find(|row| text(&field(row, "case")) == case.name)
    .unwrap();
    assert_json(
        &observed.normalized,
        &expected,
        "same unchanged original year10000 workload under actual publisher reset",
    );
    assert_artifacts(&output, &expected, &observed.public_keys, &case.input);
    let schedule: serde_json::Value = serde_json::from_slice(
        &fs::read(output.join("collector/publisher-schedule.json")).unwrap(),
    )
    .unwrap();
    let before = schedule["before_publisher_header"].as_array().unwrap();
    let reset = schedule["after_publisher_header"].as_array().unwrap();
    let refreshed = schedule["after_reader_refresh_header"].as_array().unwrap();
    assert!(before[0].as_u64().unwrap() >= 2);
    assert_eq!(before[2], 0);
    assert!(before[83..].iter().any(|v| v.as_u64().unwrap() != 0));
    assert_eq!(reset[0], 0);
    assert_ne!(reset[2], 0);
    assert!(reset[83..].iter().all(|v| v.as_u64().unwrap() == 0));
    assert!(refreshed[0].as_u64().unwrap() > 0);
    assert_eq!(refreshed[2], reset[2]);
    assert!(refreshed[83..].iter().any(|v| v.as_u64().unwrap() != 0));
    let subscribed = schedule["subscriber_registered_seconds"].as_f64().unwrap();
    let released = schedule["release_seconds"].as_f64().unwrap();
    let acknowledged = schedule["publisher_ack_seconds"].as_f64().unwrap();
    let refreshed_at = schedule["reader_refresh_seconds"].as_f64().unwrap();
    assert!(released - subscribed >= 0.8);
    assert!(acknowledged >= released);
    assert!(refreshed_at >= acknowledged);
}
