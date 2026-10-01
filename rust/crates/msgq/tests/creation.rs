use openpilot_msgq::{Publisher, Subscriber};
use std::{
    env, fs,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};

fn isolated_child(name: &str) -> bool {
    if env::var_os("MSGQ_CREATION_CHILD").is_some() {
        return true;
    }
    let namespace = tempfile::Builder::new()
        .prefix("msgq_rust-probe-creation-")
        .tempdir_in("/dev/shm")
        .unwrap();
    let prefix = namespace.path().file_name().unwrap().to_str().unwrap();
    let status = Command::new(env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("MSGQ_CREATION_CHILD", "1")
        .env("OPENPILOT_PREFIX", prefix.strip_prefix("msgq_").unwrap())
        .env_remove("CEREAL_FAKE")
        .env_remove("ZMQ")
        .status()
        .unwrap();
    assert!(status.success());
    false
}

fn path(endpoint: &str) -> PathBuf {
    PathBuf::from(format!(
        "/dev/shm/msgq_{}",
        env::var("OPENPILOT_PREFIX").unwrap()
    ))
    .join(endpoint)
}

#[test]
fn empty_creation_phase_preserves_native_interoperation() {
    if !isolated_child("empty_creation_phase_preserves_native_interoperation") {
        return;
    }
    fs::File::create(path("rustToNative")).unwrap();
    fs::File::create(path("nativeToRust")).unwrap();
    let mut outgoing = Publisher::for_runtime("rustToNative", 1024 * 1024).unwrap();
    let mut peer = Command::new(env!("NATIVE_MSGQ_PEER"))
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut output = BufReader::new(peer.stdout.take().unwrap());
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "READY");
    let mut incoming = Subscriber::for_runtime("nativeToRust", false, 1024 * 1024).unwrap();
    outgoing.send(b"queue creation\0payload").unwrap();
    line.clear();
    output.read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "SENT");
    assert_eq!(
        incoming.receive(Duration::from_secs(1)).unwrap(),
        Some(b"queue creation\0payload".to_vec())
    );
    assert!(peer.wait().unwrap().success());
}

#[test]
fn subscriber_can_initialize_a_newly_created_empty_queue() {
    if !isolated_child("subscriber_can_initialize_a_newly_created_empty_queue") {
        return;
    }
    fs::File::create(path("subscriberFirst")).unwrap();
    let mut subscriber = Subscriber::for_runtime("subscriberFirst", false, 4096).unwrap();
    let mut publisher = Publisher::for_runtime("subscriberFirst", 4096).unwrap();
    assert_eq!(subscriber.receive(Duration::ZERO).unwrap(), None);
    publisher.send(b"subscriber first").unwrap();
    assert_eq!(
        subscriber.receive(Duration::from_secs(1)).unwrap(),
        Some(b"subscriber first".to_vec())
    );
}

#[test]
fn incompatible_initialized_queues_and_directories_remain_rejected() {
    if !isolated_child("incompatible_initialized_queues_and_directories_remain_rejected") {
        return;
    }
    fs::write(path("shortQueue"), b"incompatible").unwrap();
    assert!(Subscriber::for_runtime("shortQueue", false, 4096).is_err());
    assert!(Publisher::for_runtime("shortQueue", 4096).is_err());
    assert_eq!(fs::read(path("shortQueue")).unwrap(), b"incompatible");
    fs::create_dir(path("directoryQueue")).unwrap();
    assert!(Subscriber::for_runtime("directoryQueue", false, 4096).is_err());
    assert!(Publisher::for_runtime("directoryQueue", 4096).is_err());
    let _publisher = Publisher::for_runtime("sizedQueue", 4096).unwrap();
    assert!(Subscriber::for_runtime("sizedQueue", false, 8192).is_err());
}
