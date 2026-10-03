mod support;

use openpilot_msgq::{MultiSubscriber, Publisher, Subscriber, Subscription};
use std::{
    env,
    io::{BufRead, BufReader},
    process::Stdio,
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
        let status = support::command(env::current_exe().unwrap())
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
    let mut transient = Publisher::transient_for_runtime("preview", 1024 * 1024).unwrap();
    assert!(transient.send_if_current(b"preview").unwrap());
    let mut daemon = Publisher::new("preview").unwrap();
    assert!(!transient.send_if_current(b"stale preview").unwrap());
    assert!(Publisher::transient_for_runtime("preview", 1024 * 1024).is_err());
    assert!(daemon.send_if_current(b"daemon").unwrap());
    drop(daemon);
    drop(transient);
    let mut publisher = Publisher::new("procLog").unwrap();
    assert!(Publisher::new("procLog").is_err());
    assert!(Publisher::with_capacity("procLog", 2 * 1024 * 1024).is_err());
    for capacity in [0, 1, 65 * 1024 * 1024] {
        assert!(Publisher::with_capacity("badCapacity", capacity).is_err());
    }
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
    let mut displaced = Publisher::new("nativeToRust").unwrap();
    assert!(displaced.send_if_current(b"current").unwrap());
    assert!(displaced.send_if_current(&[]).is_err());
    let mut outgoing = Publisher::new("rustToNative").unwrap();
    let mut peer = support::command(env!("NATIVE_MSGQ_PEER"))
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut ready = String::new();
    let mut peer_output = BufReader::new(peer.stdout.take().unwrap());
    peer_output.read_line(&mut ready).unwrap();
    assert_eq!(ready.trim(), "READY");
    assert!(!displaced.send_if_current(b"stale").unwrap());
    drop(displaced);
    let mut incoming = Subscriber::new("nativeToRust", false).unwrap();
    let payload: Vec<u8> = (0..=255).cycle().take(200_000).collect();
    outgoing.send(&payload).unwrap();
    let mut sent = String::new();
    peer_output.read_line(&mut sent).unwrap();
    assert_eq!(sent.trim(), "SENT");
    assert_eq!(incoming.receive(Duration::ZERO).unwrap(), Some(payload));
    assert!(peer.wait().unwrap().success());
    let specifications = [
        Subscription {
            endpoint: "pollA",
            capacity: 1024 * 1024,
            polled: true,
        },
        Subscription {
            endpoint: "pollB",
            capacity: 1024 * 1024,
            polled: true,
        },
        Subscription {
            endpoint: "aux",
            capacity: 1024 * 1024,
            polled: false,
        },
    ];
    let mut poll_a = Publisher::new("pollA").unwrap();
    let mut poll_b = Publisher::new("pollB").unwrap();
    let mut auxiliary = Publisher::new("aux").unwrap();
    let mut batch = MultiSubscriber::new(&specifications).unwrap();
    auxiliary.send(b"non-polled").unwrap();
    let started = Instant::now();
    let values = batch.receive(Duration::from_millis(30)).unwrap();
    assert!(started.elapsed() >= Duration::from_millis(25));
    assert_eq!(values.len(), 1);
    assert_eq!(
        (values[0].index, values[0].bytes.as_slice()),
        (2, b"non-polled".as_slice())
    );
    for value in 0..10_u8 {
        poll_a.send(&[value]).unwrap();
    }
    poll_b.send(b"second poll").unwrap();
    let values = batch.receive(Duration::from_millis(500)).unwrap();
    assert_eq!(values.len(), 2);
    assert_eq!(values[0].index, 0);
    assert_eq!(values[0].bytes, [9]);
    assert_eq!(values[1].index, 1);
    assert_eq!(values[1].bytes, b"second poll");
    poll_b.send(b"only one poll").unwrap();
    let started = Instant::now();
    assert_eq!(batch.receive(Duration::from_millis(500)).unwrap().len(), 1);
    assert!(started.elapsed() < Duration::from_millis(300));
    assert_eq!(values[0].bytes, [9]);
    assert!(batch.receive(Duration::ZERO).unwrap().is_empty());
    assert!(MultiSubscriber::new(&[]).is_err());
    assert!(MultiSubscriber::new(&specifications[2..]).is_err());
}

#[test]
fn production_namespace_is_refused() {
    if env::var_os("RUST_MSGQ_REFUSAL_CHILD").is_some() {
        assert!(Publisher::new("procLog").is_err());
        return;
    }
    for prefix in ["", "d", "../bad", "rust-probe-../bad"] {
        let status = support::command(env::current_exe().unwrap())
            .args(["--exact", "production_namespace_is_refused"])
            .env("RUST_MSGQ_REFUSAL_CHILD", "1")
            .env("OPENPILOT_PREFIX", prefix)
            .status()
            .unwrap();
        assert!(status.success());
    }
}

#[test]
fn explicit_runtime_transport_uses_original_namespace() {
    if env::var_os("RUST_MSGQ_RUNTIME_CHILD").is_none() {
        let namespace = tempfile::Builder::new()
            .prefix("msgq_rust-runtime-test-")
            .tempdir_in("/dev/shm")
            .unwrap();
        let name = namespace.path().file_name().unwrap().to_str().unwrap();
        let status = support::command(env::current_exe().unwrap())
            .args([
                "--exact",
                "explicit_runtime_transport_uses_original_namespace",
            ])
            .env("RUST_MSGQ_RUNTIME_CHILD", "1")
            .env("OPENPILOT_PREFIX", name.strip_prefix("msgq_").unwrap())
            .env_remove("CEREAL_FAKE")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    assert!(Publisher::new("rustToNative").is_err());
    assert!(Subscriber::new("nativeToRust", false).is_err());
    assert!(Publisher::for_runtime("../invalid", 1024 * 1024).is_err());
    let mut outgoing = Publisher::for_runtime("rustToNative", 1024 * 1024).unwrap();
    assert!(Publisher::for_runtime("rustToNative", 1024 * 1024).is_err());
    let mut peer = support::command(env!("NATIVE_MSGQ_PEER"))
        .arg("--delayed-reply")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut ready = String::new();
    let mut peer_output = BufReader::new(peer.stdout.take().unwrap());
    peer_output.read_line(&mut ready).unwrap();
    assert_eq!(ready.trim(), "READY");

    let mut incoming = Subscriber::for_runtime("nativeToRust", false, 1024 * 1024).unwrap();
    outgoing.send(b"runtime\0payload").unwrap();
    let mut sent = String::new();
    peer_output.read_line(&mut sent).unwrap();
    assert_eq!(sent.trim(), "SENT");
    assert_eq!(
        incoming.receive(Duration::ZERO).unwrap(),
        Some(b"runtime\0payload".to_vec())
    );
    assert!(peer.wait().unwrap().success());
}

#[test]
fn runtime_without_prefix_matches_native_flat_path() {
    if env::var_os("RUST_MSGQ_FLAT_CHILD").is_none() {
        let status = support::command(env::current_exe().unwrap())
            .args(["--exact", "runtime_without_prefix_matches_native_flat_path"])
            .env("RUST_MSGQ_FLAT_CHILD", "1")
            .env_remove("OPENPILOT_PREFIX")
            .env_remove("CEREAL_FAKE")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    let endpoint = format!("rustRuntimeTest{}", std::process::id());
    let path = format!("/dev/shm/msgq_{endpoint}");
    assert!(!std::path::Path::new(&path).exists());
    {
        let mut publisher = Publisher::for_runtime(&endpoint, 4096).unwrap();
        let mut subscriber = Subscriber::for_runtime(&endpoint, false, 4096).unwrap();
        publisher.send(b"flat native namespace").unwrap();
        assert_eq!(
            subscriber.receive(Duration::from_secs(1)).unwrap(),
            Some(b"flat native namespace".to_vec())
        );
        assert!(std::path::Path::new(&path).is_file());
        assert!(Publisher::new(&endpoint).is_err());
    }
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_file(format!("{path}.rust-publisher-lock")).unwrap();
}
