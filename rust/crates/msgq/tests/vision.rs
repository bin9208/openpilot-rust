#![cfg(feature = "native-skip-miri")]

use openpilot_msgq::{VisionClient, VisionStream};
use std::{
    env,
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Peer {
    process: Child,
    output: BufReader<std::process::ChildStdout>,
}

impl Peer {
    fn start() -> Self {
        let mut process = Command::new(env!("NATIVE_VISION_PEER"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let output = BufReader::new(process.stdout.take().unwrap());
        let mut peer = Self { process, output };
        peer.expect("READY");
        peer
    }

    fn expect(&mut self, expected: &str) {
        loop {
            let mut line = String::new();
            assert_ne!(self.output.read_line(&mut line).unwrap(), 0);
            if line.starts_with("Starting listener for:")
                || line.starts_with("Stopping listener for:")
            {
                continue;
            }
            assert_eq!(line.trim(), expected);
            break;
        }
    }

    fn command(&mut self, command: &str) {
        writeln!(self.process.stdin.as_mut().unwrap(), "{command}").unwrap();
        self.expect("OK");
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        self.process.kill().ok();
        self.process.wait().unwrap();
    }
}

#[test]
fn original_server_camera_transport() {
    if env::var_os("RUST_VISION_TEST_CHILD").is_none() {
        let namespace = tempfile::Builder::new()
            .prefix("msgq_rust-probe-v-")
            .tempdir_in("/dev/shm")
            .unwrap();
        let prefix = namespace.path().file_name().unwrap().to_str().unwrap();
        let status = Command::new(env::current_exe().unwrap())
            .args(["--exact", "original_server_camera_transport", "--nocapture"])
            .env("RUST_VISION_TEST_CHILD", "1")
            .env("OPENPILOT_PREFIX", prefix.strip_prefix("msgq_").unwrap())
            .env_remove("CEREAL_FAKE")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }

    assert!(VisionClient::available_streams("missing")
        .unwrap()
        .is_empty());
    let mut missing = VisionClient::new("missing", VisionStream::Road, false).unwrap();
    assert!(missing.layout().is_none());
    assert!(!missing.connect().unwrap());
    assert!(missing.receive(Duration::ZERO).is_err());
    for name in ["", "../bad", "a/b", "a\0b"] {
        assert!(VisionClient::new(name, VisionStream::Road, false).is_err());
    }

    let mut peer = Peer::start();
    assert_eq!(
        VisionClient::available_streams("rustvision").unwrap(),
        vec![
            VisionStream::Road,
            VisionStream::Driver,
            VisionStream::WideRoad,
            VisionStream::Map
        ]
    );
    let mut client = VisionClient::new("rustvision", VisionStream::Road, false).unwrap();
    assert!(client.layout().is_none());
    assert!(client.connect().unwrap());
    let retained_layout = client.layout().unwrap();
    assert_eq!(
        (
            retained_layout.width,
            retained_layout.height,
            retained_layout.stride,
            retained_layout.uv_offset,
            retained_layout.len
        ),
        (8, 4, 16, 64, 96)
    );
    let start = Instant::now();
    assert!(client.receive(Duration::from_millis(20)).unwrap().is_none());
    assert!(start.elapsed() >= Duration::from_millis(15));
    assert!(client.receive(Duration::MAX).is_err());

    peer.command("send 7");
    assert_eq!(client.layout(), Some(retained_layout));
    let frame = client.receive(Duration::from_secs(2)).unwrap().unwrap();
    let meta = frame.metadata();
    assert_eq!(
        (
            meta.width,
            meta.height,
            meta.stride,
            meta.uv_offset,
            meta.len
        ),
        (8, 4, 16, 64, 96)
    );
    assert_eq!(
        (
            meta.frame_id,
            meta.timestamp_sof,
            meta.timestamp_eof,
            meta.valid
        ),
        (7, 7000, 7100, false)
    );
    let mut retained = vec![0; meta.len];
    assert!(frame.copy_into(&mut retained[..95]).is_err());
    frame.copy_into(&mut retained).unwrap();
    assert_eq!(
        retained,
        (0_u8..96)
            .map(|index| index.wrapping_add(7))
            .collect::<Vec<_>>()
    );

    let mut latest = VisionClient::new("rustvision", VisionStream::Road, true).unwrap();
    assert!(latest.connect().unwrap());
    peer.command("send 8");
    peer.command("send 9");
    assert_eq!(
        client
            .receive(Duration::from_secs(2))
            .unwrap()
            .unwrap()
            .metadata()
            .frame_id,
        8
    );
    assert_eq!(
        client
            .receive(Duration::from_secs(2))
            .unwrap()
            .unwrap()
            .metadata()
            .frame_id,
        9
    );
    assert_eq!(
        latest
            .receive(Duration::from_secs(2))
            .unwrap()
            .unwrap()
            .metadata()
            .frame_id,
        9
    );
    drop(latest);

    peer.command("restart");
    for _ in 0..10 {
        peer.command("send 10");
        assert!(client.receive(Duration::from_millis(50)).unwrap().is_none());
        if !client.is_connected() {
            break;
        }
    }
    assert!(!client.is_connected());
    assert!(client.connect().unwrap());
    peer.command("send 11");
    assert_eq!(
        client
            .receive(Duration::from_secs(2))
            .unwrap()
            .unwrap()
            .metadata()
            .frame_id,
        11
    );
    drop(client);
    assert_eq!(retained[0], 7);
    assert_eq!(retained_layout.len, 96);

    for stream in [
        VisionStream::Driver,
        VisionStream::WideRoad,
        VisionStream::Map,
    ] {
        let mut stream_client = VisionClient::new("rustvision", stream, false).unwrap();
        assert!(stream_client.connect().unwrap());
        peer.command("send 12");
        let frame = stream_client
            .receive(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        assert_eq!(frame.metadata().frame_id, 12);
        assert!(frame.metadata().valid);
    }
    let fds = std::fs::read_dir("/proc/self/fd").unwrap().count();
    for _ in 0..30 {
        let mut temporary = VisionClient::new("rustvision", VisionStream::Road, false).unwrap();
        assert!(temporary.connect().unwrap());
        assert!(temporary.connect().unwrap());
    }
    assert_eq!(std::fs::read_dir("/proc/self/fd").unwrap().count(), fds);
    peer.command("invalid-layout");
    let mut invalid = VisionClient::new("rustvision", VisionStream::Road, false).unwrap();
    assert!(invalid.connect().is_err());
    assert!(!invalid.is_connected());
    assert!(invalid.receive(Duration::ZERO).is_err());
    peer.command("restart");
    assert!(invalid.connect().unwrap());
    peer.command("stop");
    assert!(peer.process.wait().unwrap().success());
}
