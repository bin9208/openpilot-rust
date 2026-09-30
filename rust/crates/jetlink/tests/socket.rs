use openpilot_jetlink::{
    contract::{self, Identity},
    owner::{Backend, Server},
    rpc::{self, ProxyClient},
    Deadline, Error,
};
use std::{
    os::unix::net::{UnixListener, UnixStream},
    sync::{atomic::Ordering, mpsc, Mutex},
    thread,
    time::{Duration, Instant},
};
// Keep independent socket scenarios from sharing the strict 50 ms frame budget under ASan.
static FIXTURE_LOCK: Mutex<()> = Mutex::new(());

struct Synthetic {
    last: Option<u32>,
    dead: bool,
}
impl Backend for Synthetic {
    fn connect(&mut self) -> Result<Identity, Error> {
        Ok(Identity::new())
    }
    fn infer(
        &mut self,
        frame: u32,
        _: &[u8],
        _: &[f32],
        _: Deadline,
        reset: bool,
    ) -> Result<Vec<f32>, Error> {
        if !reset && self.last.is_some_and(|last| frame <= last) {
            self.dead = true;
            return Err(Error::Contract("frame order"));
        }
        self.last = Some(frame);
        Ok(vec![0.25; contract::OUTPUT_FLOATS])
    }
    fn dead(&self) -> bool {
        self.dead
    }
    fn close(&mut self) {
        self.dead = true;
    }
}
fn ready(server: &Server) {
    let end = Instant::now() + Duration::from_secs(2);
    while !server.ready.load(Ordering::Acquire) {
        assert!(Instant::now() < end);
        thread::yield_now();
    }
}
#[test]
fn native_socket_returns_current_frame_and_rejects_reordered_frame() {
    let _serial = FIXTURE_LOCK.lock().unwrap();
    // Given a real local owner and an otherwise successful hardware-facing backend.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jetlink.sock");
    let mut server = Server::spawn(
        Synthetic {
            last: None,
            dead: false,
        },
        &path,
    )
    .unwrap();
    ready(&server);
    let mut client = ProxyClient::connect(&path).unwrap();
    let image = vec![0; contract::WARPED_BYTES];
    // When a frame succeeds and the caller reuses its frame identity.
    let output = client
        .infer(
            42,
            &image,
            &[0.0; 12],
            Deadline::after(Duration::from_millis(50)).unwrap(),
            true,
        )
        .unwrap();
    let result = client.infer(
        42,
        &image,
        &[0.0; 12],
        Deadline::after(Duration::from_millis(50)).unwrap(),
        false,
    );
    // Then actual socket output matches the fixture and the failed stream is permanently closed.
    assert_eq!(output, vec![0.25; contract::OUTPUT_FLOATS]);
    assert!(result.is_err());
    assert!(client.dead());
    server.stop(Duration::from_secs(1)).unwrap();
    assert!(!server.error.lock().unwrap().is_empty());
}
#[test]
fn malformed_generation_does_not_destroy_idle_owner() {
    let _serial = FIXTURE_LOCK.lock().unwrap();
    // Given a real connected owner.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jetlink.sock");
    let mut server = Server::spawn(
        Synthetic {
            last: None,
            dead: false,
        },
        &path,
    )
    .unwrap();
    ready(&server);
    let client = ProxyClient::connect(&path).unwrap();
    let mut stream = client.cancel_handle().unwrap();
    // When a malformed owner generation arrives, then a new correctly identified client connects.
    let deadline = Deadline::after(Duration::from_millis(50)).unwrap();
    let request = rpc::encode_request(
        [0; 16],
        1,
        deadline,
        true,
        &vec![0; contract::WARPED_BYTES],
        &[0.0; 12],
    );
    rpc::send_packet(&mut stream, &request, deadline).unwrap();
    assert!(rpc::recv_packet(&mut stream, deadline).is_err());
    drop(client);
    drop(stream);
    let mut client = ProxyClient::connect(&path).unwrap();
    // Then the owner is still ready and serves a valid current frame.
    assert_eq!(
        client
            .infer(
                1,
                &vec![0; contract::WARPED_BYTES],
                &[0.0; 12],
                Deadline::after(Duration::from_millis(50)).unwrap(),
                true
            )
            .unwrap()
            .len(),
        contract::OUTPUT_FLOATS
    );
    client.close();
    server.stop(Duration::from_secs(1)).unwrap();
}
#[test]
fn partial_packet_cannot_extend_whole_deadline() {
    let _serial = FIXTURE_LOCK.lock().unwrap();
    // Given only part of a declared packet on a real socket.
    let (mut receiver, mut sender) = UnixStream::pair().unwrap();
    use std::io::Write;
    sender.write_all(&[8, 0, 0, 0, 1]).unwrap();
    // When the receiver waits for the missing bytes.
    let started = Instant::now();
    let result = rpc::recv_packet(
        &mut receiver,
        Deadline::after(Duration::from_millis(20)).unwrap(),
    );
    // Then it returns an error within a bounded host scheduling allowance.
    assert!(result.is_err());
    assert!(started.elapsed() < Duration::from_millis(300));
}
#[test]
fn oversize_packet_is_rejected_before_payload_allocation() {
    let _serial = FIXTURE_LOCK.lock().unwrap();
    // Given a hostile packet size.
    let (mut receiver, mut sender) = UnixStream::pair().unwrap();
    use std::io::Write;
    sender.write_all(&u32::MAX.to_le_bytes()).unwrap();
    // When decoded, then the framing limit rejects it.
    assert!(matches!(
        rpc::recv_packet(
            &mut receiver,
            Deadline::after(Duration::from_millis(50)).unwrap()
        ),
        Err(Error::Contract("packet length"))
    ));
}
#[test]
fn stop_interrupts_waiting_local_client_and_joins_worker() {
    let _serial = FIXTURE_LOCK.lock().unwrap();
    // Given a client stalled before its handshake.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jetlink.sock");
    let mut server = Server::spawn(
        Synthetic {
            last: None,
            dead: false,
        },
        &path,
    )
    .unwrap();
    ready(&server);
    let _stream = UnixStream::connect(&path).unwrap();
    // When stopping the owner, then no client traffic is needed to join.
    let start = Instant::now();
    server.stop(Duration::from_secs(1)).unwrap();
    assert!(start.elapsed() < Duration::from_millis(700));
    assert!(!path.exists());
}
#[test]
fn wrong_reply_identity_closes_proxy() {
    let _serial = FIXTURE_LOCK.lock().unwrap();
    // Given a hardware-independent real socket peer that handshakes correctly.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jetlink.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let (tx, rx) = mpsc::channel();
    let peer = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let deadline = Deadline::after(Duration::from_secs(1)).unwrap();
        assert_eq!(rpc::recv_packet(&mut stream, deadline).unwrap(), b"H");
        let mut hello = vec![b'J'];
        hello.extend(
            serde_json::to_vec(&rpc::Hello {
                ready: true,
                generation: rpc::encode_generation([3; 16]),
                spec: contract::contract().unwrap(),
                identity: Identity::new(),
            })
            .unwrap(),
        );
        rpc::send_packet(&mut stream, &hello, deadline).unwrap();
        rpc::recv_packet(&mut stream, deadline).unwrap();
        let mut response = vec![0; 21 + contract::OUTPUT_FLOATS * 4];
        response[0] = b'R';
        response[1..17].copy_from_slice(&[4; 16]);
        rpc::send_packet(&mut stream, &response, deadline).unwrap();
        tx.send(()).unwrap();
    });
    // When the peer replies with a stale generation.
    let mut client = ProxyClient::connect(&path).unwrap();
    let result = client.infer(
        0,
        &vec![0; contract::WARPED_BYTES],
        &[0.0; 12],
        Deadline::after(Duration::from_millis(50)).unwrap(),
        true,
    );
    // Then no output is accepted and the proxy cannot be reused.
    assert!(result.is_err());
    assert!(client.dead());
    rx.recv_timeout(Duration::from_secs(1)).unwrap();
    peer.join().unwrap();
}

#[test]
fn saturated_listener_does_not_make_connect_unbounded() {
    let _serial = FIXTURE_LOCK.lock().unwrap();
    // Given a real Unix listener whose accept queue is deliberately full.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("busy.sock");
    let socket = rustix::net::socket_with(
        rustix::net::AddressFamily::UNIX,
        rustix::net::SocketType::STREAM,
        rustix::net::SocketFlags::CLOEXEC,
        None,
    )
    .unwrap();
    let address = rustix::net::SocketAddrUnix::new(&path).unwrap();
    rustix::net::bind(&socket, &address).unwrap();
    rustix::net::listen(&socket, 0).unwrap();
    let _queued = UnixStream::connect(&path).unwrap();
    // When the proxy attempts to connect with the original 500ms complete handshake budget.
    let start = Instant::now();
    let result = ProxyClient::connect(&path);
    // Then it expires instead of blocking indefinitely inside connect(2).
    assert!(result.is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn kernel_stuck_hardware_seam_cannot_extend_explicit_owner_stop() {
    let _serial = FIXTURE_LOCK.lock().unwrap();
    struct Blocked {
        entered: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    }
    impl Backend for Blocked {
        fn connect(&mut self) -> Result<Identity, Error> {
            self.entered.send(()).unwrap();
            self.release.recv().unwrap();
            Ok(Identity::new())
        }
        fn infer(
            &mut self,
            _: u32,
            _: &[u8],
            _: &[f32],
            _: Deadline,
            _: bool,
        ) -> Result<Vec<f32>, Error> {
            Err(Error::Closed)
        }
        fn dead(&self) -> bool {
            false
        }
        fn close(&mut self) {}
    }
    // Given a hardware connection that remains blocked independently of local sockets.
    let dir = tempfile::tempdir().unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let mut server = Server::spawn(
        Blocked {
            entered: entered_tx,
            release: release_rx,
        },
        &dir.path().join("blocked.sock"),
    )
    .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    // When the owner stops and is then dropped, the bound must not restart in Drop.
    let start = Instant::now();
    assert!(matches!(
        server.stop(Duration::from_millis(20)),
        Err(Error::Deadline)
    ));
    drop(server);
    // Then the caller returns without unblocking hardware or waiting for a second shutdown timeout.
    assert!(start.elapsed() < Duration::from_millis(200));
    release_tx.send(()).unwrap();
}
