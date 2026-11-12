use super::*;

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
