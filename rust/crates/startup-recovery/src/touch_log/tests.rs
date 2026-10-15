use super::*;
use serde_json::json;
fn snapshot(path: &Path) -> serde_json::Value {
    if !path.exists() {
        return json!({"kind":"missing"});
    }
    if path.is_dir() {
        return json!({"kind":"directory"});
    }
    json!({"kind":"file","hex":fs::read(path).unwrap().iter().map(|b|format!("{b:02x}")).collect::<String>()})
}
#[test]
fn original_rotation_bytes_surrogates_nonfinite_and_failure_order() {
    let rows: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../fixtures/startup-recovery-touch-log-final.json"
    ))
    .unwrap();
    for row in rows.as_array().unwrap() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(row["name"].as_str().unwrap());
        match row["initial"].as_str().unwrap() {
            "directory" => fs::create_dir(&path).unwrap(),
            "file" => fs::write(&path, b"old incomplete\n").unwrap(),
            _ => {}
        }
        let logger = TouchLog::new(
            path.clone(),
            row["limit"].as_u64().unwrap(),
            "owned-boot".into(),
        );
        for step in row["steps"].as_array().unwrap() {
            let Value::Object(values) =
                Value::parse(step["fields_json"].as_str().unwrap()).unwrap()
            else {
                panic!("fields");
            };
            let fields: Vec<_> = values
                .into_iter()
                .map(|(key, value)| (Value::Text(key).string().unwrap(), value))
                .collect();
            let borrowed: Vec<_> = fields
                .iter()
                .map(|(key, value)| (key.as_str(), value.clone()))
                .collect();
            assert_eq!(
                logger.record_with("press", step["sync"].as_bool().unwrap(), &borrowed, || (
                    step["wall"].as_f64().unwrap(),
                    step["mono"].as_f64().unwrap()
                )),
                step["ok"].as_bool().unwrap()
            );
            assert_eq!(snapshot(&path), step["main"]);
            assert_eq!(
                snapshot(&root.path().join(row["archive_name"].as_str().unwrap())),
                step["archive"]
            );
        }
    }
}

#[test]
fn independent_writers_serialize_rotation_and_busy_logging_is_bounded() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("events");
    fs::write(&path, "x".repeat(90)).unwrap();
    let mut first = TouchLog::for_path(path.clone());
    first.max_bytes = 100;
    let mut second = TouchLog::for_path(path.clone());
    second.max_bytes = 100;
    let (opened, ready) = std::sync::mpsc::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let writer = std::thread::spawn(move || {
        first.record_with("first", false, &[], || {
            opened.send(()).unwrap();
            wait.recv().unwrap();
            (0.0, 0.0)
        })
    });
    ready.recv().unwrap();
    // Another instance/process cannot rotate the first writer's open file.
    let started = Instant::now();
    assert!(!second.record_with("blocked", false, &[], || panic!("busy logger read clock")));
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(fs::read(&path).unwrap(), vec![b'x'; 90]);
    release.send(()).unwrap();
    assert!(writer.join().unwrap());
    assert!(second.record_with("second", false, &[], || (0.0, 0.0)));
    let main = fs::read_to_string(&path).unwrap();
    let archive = fs::read_to_string(root.path().join("events.1")).unwrap();
    assert!(archive.starts_with(&"x".repeat(90)) && archive.contains("first"));
    assert!(main.contains("second") && !main.contains("first"));
}
