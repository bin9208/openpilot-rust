use super::store::*;
use openpilot_logging::{Fields, Value};
use std::{fs, io};
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
