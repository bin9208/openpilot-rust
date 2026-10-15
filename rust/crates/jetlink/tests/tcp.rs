use openpilot_jetlink::{
    client::Transport,
    tcp::TcpTransport,
    wire::{self, Version},
    Deadline,
};
use std::{
    io::Write,
    net::TcpListener,
    time::{Duration, Instant},
};
#[test]
fn tcp_uses_selected_protocol_short_packet_padding_and_whole_deadline() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let peer = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut peer = TcpTransport::from_stream(stream, Version::V3).unwrap();
        let request = peer
            .receive(Deadline::after(Duration::from_secs(1)).unwrap())
            .unwrap();
        assert_eq!(request.header.kind, 8);
        assert_eq!(request.payload.len(), 480);
        assert_eq!(request.header.flags, wire::PADDED);
        peer.send(
            &wire::frame_version(9, request.header.sequence, b"done", false, Version::V3).unwrap(),
            Deadline::after(Duration::from_secs(1)).unwrap(),
        )
        .unwrap();
    });
    let mut client = TcpTransport::connect(
        address,
        Version::V3,
        Deadline::after(Duration::from_secs(1)).unwrap(),
    )
    .unwrap();
    assert!(!client.gadget());
    client
        .send(
            &wire::frame_version(8, 7, &[0; 480], false, Version::V3).unwrap(),
            Deadline::after(Duration::from_secs(1)).unwrap(),
        )
        .unwrap();
    assert_eq!(
        client
            .receive(Deadline::after(Duration::from_secs(1)).unwrap())
            .unwrap()
            .payload,
        b"done"
    );
    peer.join().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let peer = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.write_all(b"J").unwrap();
        std::thread::sleep(Duration::from_millis(100));
    });
    let mut client = TcpTransport::connect(
        address,
        Version::V3,
        Deadline::after(Duration::from_secs(1)).unwrap(),
    )
    .unwrap();
    let started = Instant::now();
    assert!(client
        .receive(Deadline::after(Duration::from_millis(20)).unwrap())
        .is_err());
    assert!(started.elapsed() < Duration::from_millis(90));
    peer.join().unwrap();
}
#[test]
fn transport_mode_override_file_and_corruption_match_source() {
    use openpilot_jetlink::mobile::config::Mode;
    use std::{ffi::OsStr, fs};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mode");
    assert_eq!(Mode::read(None, &path).unwrap(), Mode::Auto);
    fs::write(&path, "ios\n").unwrap();
    assert_eq!(Mode::read(None, &path).unwrap(), Mode::Ios);
    assert_eq!(
        Mode::read(Some(OsStr::new("android")), &path).unwrap(),
        Mode::Android
    );
    for value in ["", "auto ", "wifi", "IOS"] {
        assert!(Mode::read(Some(OsStr::new(value)), &path).is_err());
    }
    fs::write(&path, "bad").unwrap();
    assert!(Mode::read(None, &path).is_err());
    assert!(Mode::read(None, dir.path()).is_err());
}

#[test]
fn preparation_poll_retains_partial_tcp_header_until_next_deadline() {
    use std::sync::mpsc;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let bytes = wire::frame_version(13, 7, b"{\"loaded\":null}", false, Version::V3).unwrap();
    let (sent, received) = mpsc::sync_channel(1);
    let (resume, proceed) = mpsc::sync_channel(1);
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.write_all(&bytes[..17]).unwrap();
        sent.send(()).unwrap();
        proceed.recv().unwrap();
        stream.write_all(&bytes[17..]).unwrap();
    });
    let mut transport = TcpTransport::connect(
        address,
        Version::V3,
        Deadline::after(Duration::from_secs(1)).unwrap(),
    )
    .unwrap();
    received.recv().unwrap();
    assert!(matches!(
        transport.receive(Deadline::after(Duration::from_millis(5)).unwrap()),
        Err(openpilot_jetlink::Error::Deadline)
    ));
    resume.send(()).unwrap();
    let response = transport
        .receive(Deadline::after(Duration::from_secs(1)).unwrap())
        .unwrap();
    assert_eq!(response.header.sequence, 7);
    assert_eq!(response.payload, b"{\"loaded\":null}");
    server.join().unwrap();
}
