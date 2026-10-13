#![cfg(feature = "native")]
use openpilot_carrot_navi::{manifest::MapConfig, native::EmbeddedServer};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    time::{Duration, Instant},
};

#[test]
fn embedded_listener_serves_and_releases_its_owned_port() {
    let mut server = EmbeddedServer::start("127.0.0.1", 0, MapConfig::default(), false).unwrap();
    let address = server.local_addr();
    let mut peer = TcpStream::connect(address).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    peer.write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut reply = String::new();
    peer.read_to_string(&mut reply).unwrap();
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert!(reply.contains("carrot_navi_receiver"));
    assert!(!server
        .snapshot()
        .unwrap()
        .0
        .metadata
        .get("connected")
        .truth());
    let idle = TcpStream::connect(address).unwrap();
    let start = Instant::now();
    server.close().unwrap();
    assert!(start.elapsed() < Duration::from_secs(3));
    server.close().unwrap();
    drop(idle);
    drop(peer);
    let listener = TcpListener::bind(address).unwrap();
    drop(listener);
}

#[test]
fn occupied_port_reports_startup_failure_without_a_worker() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    assert!(EmbeddedServer::start("127.0.0.1", port, MapConfig::default(), false).is_err());
}

#[test]
fn unresponsive_upgraded_peer_cannot_keep_the_embedded_worker_alive() {
    let mut server = EmbeddedServer::start("127.0.0.1", 0, MapConfig::default(), false).unwrap();
    let address = server.local_addr();
    let mut peer = TcpStream::connect(address).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    peer.write_all(b"GET /api/navi/ws/v2/control/fixture-app HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n").unwrap();
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        peer.read_exact(&mut byte).unwrap();
        header.push(byte[0]);
        assert!(header.len() < 8192);
    }
    assert!(header.starts_with(b"HTTP/1.1 101"), "{header:?}");
    let start = Instant::now();
    server.close().unwrap();
    assert!(start.elapsed() < Duration::from_secs(3));
    let mut remaining = Vec::new();
    peer.read_to_end(&mut remaining).unwrap();
    drop(peer);
    drop(TcpListener::bind(address).unwrap());
}
