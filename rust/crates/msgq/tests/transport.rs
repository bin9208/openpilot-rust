use openpilot_msgq::{Publisher, Subscriber};
use std::{
    env,
    io::{BufRead, BufReader},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn isolated_transport() {
    if env::var_os("RUST_MSGQ_TEST_CHILD").is_none() {
        let namespace = tempfile::Builder::new()
            .prefix("msgq_rust-probe-")
            .tempdir_in("/dev/shm")
            .unwrap();
        let name = namespace
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .strip_prefix("msgq_")
            .unwrap();
        let status = Command::new(env::current_exe().unwrap())
            .args(["--exact", "isolated_transport", "--nocapture"])
            .env("RUST_MSGQ_TEST_CHILD", "1")
            .env("OPENPILOT_PREFIX", name)
            .env_remove("CEREAL_FAKE")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    for endpoint in ["", "../procLog", "/procLog", "a/b", "a\0b"] {
        assert!(Publisher::new(endpoint).is_err());
    }
    let mut publisher = Publisher::new("procLog").unwrap();
    let mut subscriber = Subscriber::new("procLog", false).unwrap();
    let start = Instant::now();
    assert_eq!(subscriber.receive(Duration::from_millis(20)).unwrap(), None);
    assert!(start.elapsed() >= Duration::from_millis(15));
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(publisher.send(&[]).is_err());
    assert!(publisher.send(&vec![1; 1_000_000]).is_err());
    let payload = [0, 255, 1, 2, 0, 3];
    publisher.send(&payload).unwrap();
    assert_eq!(
        subscriber.receive(Duration::from_millis(100)).unwrap(),
        Some(payload.to_vec())
    );
    let mut conflated = Subscriber::new("procLog", true).unwrap();
    for value in 0_u32..1000 {
        publisher.send(&value.to_le_bytes()).unwrap();
    }
    assert_eq!(
        conflated.receive(Duration::from_millis(100)).unwrap(),
        Some(999_u32.to_le_bytes().to_vec())
    );
    let retained = subscriber.receive(Duration::ZERO).unwrap().unwrap();
    drop(publisher);
    let mut publisher = Publisher::new("procLog").unwrap();
    subscriber.receive(Duration::ZERO).unwrap();
    publisher.send(b"reconnected").unwrap();
    assert_eq!(
        subscriber.receive(Duration::from_millis(100)).unwrap(),
        Some(b"reconnected".to_vec())
    );
    assert_eq!(retained, 0_u32.to_le_bytes());
    for _ in 0..100 {
        let mut temporary = Subscriber::new("procLog", true).unwrap();
        assert_eq!(temporary.receive(Duration::ZERO).unwrap(), None);
    }
    let mut outgoing = Publisher::new("rustToNative").unwrap();
    let mut peer = Command::new(env!("NATIVE_MSGQ_PEER"))
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut ready = String::new();
    BufReader::new(peer.stdout.take().unwrap())
        .read_line(&mut ready)
        .unwrap();
    assert_eq!(ready.trim(), "READY");
    let mut incoming = Subscriber::new("nativeToRust", false).unwrap();
    let payload: Vec<u8> = (0..=255).cycle().take(200_000).collect();
    outgoing.send(&payload).unwrap();
    assert_eq!(
        incoming.receive(Duration::from_secs(2)).unwrap(),
        Some(payload)
    );
    assert!(peer.wait().unwrap().success());
}

#[test]
fn production_namespace_is_refused() {
    if env::var_os("RUST_MSGQ_REFUSAL_CHILD").is_some() {
        assert!(Publisher::new("procLog").is_err());
        return;
    }
    for prefix in ["", "d", "../bad", "rust-probe-../bad"] {
        let status = Command::new(env::current_exe().unwrap())
            .args(["--exact", "production_namespace_is_refused"])
            .env("RUST_MSGQ_REFUSAL_CHILD", "1")
            .env("OPENPILOT_PREFIX", prefix)
            .status()
            .unwrap();
        assert!(status.success());
    }
}
