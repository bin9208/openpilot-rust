use super::store::*;
use openpilot_logging::{Fields, Value};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{cell::RefCell, fs, io};
#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    event: Value,
    large: bool,
    payload_error: bool,
    error: Option<String>,
    calls: Vec<Value>,
    reports: Vec<Report>,
}
#[derive(Deserialize)]
struct Report {
    file: String,
    report: Value,
    text_sha256: String,
    text_bytes: usize,
}
#[test]
fn final_source_store_106_cases_exact_metadata_text_and_call_order() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../../../fixtures/process-crash-final.json"
    ))
    .unwrap();
    for (index, case) in fixture.cases.into_iter().enumerate() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::new(directory.path().into());
        let Value::Object(event) = case.event else {
            panic!("fixture event object")
        };
        let calls = RefCell::new(Vec::new());
        let prepared = Prepared::capture(
            event,
            1700000000.125,
            |event| {
                calls.borrow_mut().push(Value::Array(vec![
                    Value::Text("capture".into()),
                    Value::Object(event.clone()),
                ]));
                Ok(if case.large {
                    "한😀x".repeat(40000)
                } else {
                    "original traceback\n".into()
                })
            },
            |event| {
                calls.borrow_mut().push(Value::Array(vec![
                    Value::Text("payload".into()),
                    Value::Object(event.clone()),
                ]));
                if case.payload_error {
                    return Err(io::Error::other("owned Params unavailable"));
                }
                Ok([
                    ("tmux_why".into(), Value::Text("process_crash".into())),
                    ("car_name".into(), Value::Text("owned".into())),
                ]
                .into_iter()
                .collect())
            },
        );
        let result = prepared.and_then(|prepared| store.record(prepared));
        assert_eq!(
            result.is_err(),
            case.error.is_some(),
            "case{index}: {result:?}"
        );
        assert_eq!(
            *calls.borrow(),
            case.calls,
            "case{index} callback order/metadata"
        );
        assert_eq!(
            fs::read_dir(directory.path()).unwrap().count(),
            case.reports.len() * 2,
            "case{index}"
        );
        for report in case.reports {
            let path = directory.path().join(report.file);
            let actual = fields(&fs::read(&path).unwrap()).unwrap();
            assert_eq!(Value::Object(actual), report.report, "case{index} metadata");
            let text = fs::read(path.with_extension("txt")).unwrap();
            assert_eq!(text.len(), report.text_bytes, "case{index} text size");
            assert_eq!(
                format!("{:x}", Sha256::digest(&text)),
                report.text_sha256,
                "case{index} exact original text bytes"
            );
        }
    }
}
fn event(index: usize) -> Fields {
    fields(
        format!(
            r#"{{"id":"1-2-{index}","name":"worker","pid":2,"exitcode":-6,"git_commit":"owned"}}"#
        )
        .as_bytes(),
    )
    .unwrap()
}
fn record(store: &mut Store, index: usize, text: &str) {
    store
        .record(
            Prepared::capture(
                event(index),
                1700000000.0,
                |_| Ok(text.into()),
                |_| Err(io::Error::other("missing Params")),
            )
            .unwrap(),
        )
        .unwrap();
}
#[test]
fn retained_snapshot_and_per_destination_ack_survive_restart() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = Store::new(directory.path().into());
    record(&mut store, 1, "original traceback");
    let path = directory.path().join("1-2-1.json");
    let text = fs::read(path.with_extension("txt")).unwrap();
    assert_eq!(store.due(0.0).unwrap(), vec![path.clone()]);
    store.acknowledge(&path, "carrot_logs").unwrap();
    store.defer(&path, 0.0);
    record(&mut store, 1, "new contents must not replace evidence");
    assert_eq!(fs::read(path.with_extension("txt")).unwrap(), text);
    assert!(store.due(59.999).unwrap().is_empty());
    assert_eq!(store.due(60.0).unwrap().len(), 1);
    let mut restarted = Store::new(directory.path().into());
    assert!(restarted.acknowledged(&path, "carrot_logs").unwrap());
    assert!(!restarted.acknowledged(&path, "dsm").unwrap());
    assert_eq!(restarted.due(2.0).unwrap().len(), 1);
    restarted.acknowledge(&path, "dsm").unwrap();
    restarted.acknowledge(&path, "dsm").unwrap();
    let report = fields(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        report["sent"],
        Value::Array(vec![
            Value::Text("carrot_logs".into()),
            Value::Text("dsm".into())
        ])
    );
    assert_eq!(fs::read(path.with_extension("txt")).unwrap(), text);
}
#[test]
fn retention_caps_twenty_reports_and_deletes_old_retry_state() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = Store::new(directory.path().into());
    for index in 0..23 {
        record(&mut store, index, "traceback");
        store.defer(&directory.path().join(format!("1-2-{index}.json")), 0.0);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 40);
    assert_eq!(store.retry_at.len(), 20);
    for index in 0..3 {
        assert!(!directory.path().join(format!("1-2-{index}.json")).exists());
    }
}
