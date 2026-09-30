use openpilot_jetlink::{
    client::{Client, Transport},
    contract,
    ffs::{FfsTransport, Session},
    owner::Backend,
    rpc,
    wire::{self, Header},
    Deadline,
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
struct Fixture {
    dir: tempfile::TempDir,
    session: Arc<Session>,
    offroad: Arc<AtomicBool>,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join("udcs")).unwrap();
        fs::create_dir(root.join("udcs/test")).unwrap();
        fs::write(root.join("udcs/test/state"), "configured\n").unwrap();
        fs::write(root.join("UDC"), "").unwrap();
        fs::write(root.join("ep0"), "").unwrap();
        for name in ["ep1", "ep2"] {
            rustix::fs::mknodat(
                rustix::fs::CWD,
                root.join(name),
                rustix::fs::FileType::Fifo,
                rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
                0,
            )
            .unwrap();
        }
        let offroad = Arc::new(AtomicBool::new(true));
        let read_offroad = Arc::clone(&offroad);
        let session = Session::open(
            root,
            root,
            &root.join("udcs"),
            Arc::new(move || read_offroad.load(Ordering::Acquire)),
        )
        .unwrap();
        Self {
            dir,
            session,
            offroad,
        }
    }
    fn peer(&self) -> (File, File) {
        let read = OpenOptions::new()
            .read(true)
            .write(true)
            .open(self.dir.path().join("ep2"))
            .unwrap();
        let write = OpenOptions::new()
            .read(true)
            .write(true)
            .open(self.dir.path().join("ep1"))
            .unwrap();
        (read, write)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.offroad.store(true, Ordering::Release);
        self.session.teardown().unwrap();
        let mut wake = OpenOptions::new()
            .read(true)
            .write(true)
            .open(self.dir.path().join("ep1"))
            .unwrap();
        wake.write_all(&[0]).unwrap();
    }
}
fn receive(reader: &mut File) -> (Header, Vec<u8>) {
    let mut bytes = [0; 32];
    reader.read_exact(&mut bytes).unwrap();
    let header = Header::decode(&bytes).unwrap();
    let length = usize::try_from(header.length).unwrap();
    let pad = (16384 - (32 + length) % 16384) % 16384;
    let mut payload = vec![0; length + pad];
    reader.read_exact(&mut payload).unwrap();
    payload.truncate(length);
    (header, payload)
}
#[test]
fn native_functionfs_client_preserves_original_wire_and_model_contract() {
    // Given ordinary FIFOs as the hardware seam, with the real FunctionFS transport/client.
    let fixture = Fixture::new();
    let (mut from_client, mut to_client) = fixture.peer();
    let peer = thread::spawn(move || {
        let (header, _) = receive(&mut from_client);
        assert_eq!(header.kind, 1);
        let hello = serde_json::json!({"protocol":2,"backend":"ort","runtime_version":"1.22.0","telemetry":{"artifact_sha256":contract::SHA256,"source_sha256":contract::SHA256,"device_model":"fixture","android_api":"35","backend_requested":"cpu","app_version":"test"}});
        to_client
            .write_all(
                &wire::frame(
                    2,
                    header.sequence,
                    &serde_json::to_vec(&hello).unwrap(),
                    false,
                )
                .unwrap(),
            )
            .unwrap();
        let (header, payload) = receive(&mut from_client);
        assert_eq!(header.kind, 3);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&payload).unwrap()["sha256"],
            contract::SHA256
        );
        let state = serde_json::json!({"state":"ready","spec":contract::contract().unwrap()});
        to_client
            .write_all(
                &wire::frame(
                    4,
                    header.sequence,
                    &serde_json::to_vec(&state).unwrap(),
                    false,
                )
                .unwrap(),
            )
            .unwrap();
        let (header, payload) = receive(&mut from_client);
        assert_eq!(header.kind, 8);
        assert_eq!(wire::word(&payload, 0), 41);
        assert_eq!(wire::word(&payload, 4), 1);
        assert_eq!(payload.len(), 8 + contract::WARPED_BYTES + 48);
        assert_eq!(
            &payload[8..8 + contract::WARPED_BYTES],
            vec![13; contract::WARPED_BYTES]
        );
        assert_eq!(
            rpc::decode_floats(&payload[8 + contract::WARPED_BYTES..]).unwrap(),
            [0.125; 12]
        );
        let mut output = Vec::new();
        for value in [41u32, 0, 12, 13, 14] {
            output.extend(value.to_le_bytes());
        }
        for _ in 0..contract::OUTPUT_FLOATS {
            output.extend(0.375f32.to_le_bytes());
        }
        to_client
            .write_all(&wire::frame(9, header.sequence, &output, false).unwrap())
            .unwrap();
    });
    let mut client = Client::new(FfsTransport::new(Arc::clone(&fixture.session)));
    // When the real client handshakes and performs one reset inference.
    let identity = client.connect().unwrap();
    let output = client
        .infer(
            41,
            &vec![13; contract::WARPED_BYTES],
            &[0.125; 12],
            Deadline::after(Duration::from_millis(50)).unwrap(),
            true,
        )
        .unwrap();
    // Then the response identity, float payload and source-built descriptors are preserved.
    assert_eq!(identity["artifact_sha256"], contract::SHA256);
    assert_eq!(output, vec![0.375; contract::OUTPUT_FLOATS]);
    let mut descriptors = wire::descriptors();
    descriptors.extend(wire::strings());
    assert_eq!(
        fs::read(fixture.dir.path().join("ep0")).unwrap(),
        descriptors
    );
    client.close();
    peer.join().unwrap();
}
#[test]
fn role_teardown_is_refused_onroad() {
    // Given a configured synthetic gadget whose offroad state changes.
    let fixture = Fixture::new();
    fixture.offroad.store(false, Ordering::Release);
    // When teardown is requested, then the controller binding is unchanged.
    assert!(fixture.session.teardown().is_err());
    assert_eq!(
        fs::read_to_string(fixture.dir.path().join("UDC")).unwrap(),
        "test\n"
    );
}
#[test]
fn receive_deadline_is_independent_of_blocked_hardware_reader() {
    // Given an already enabled synthetic transport with a peer that sends nothing.
    let fixture = Fixture::new();
    let mut transport = FfsTransport::new(Arc::clone(&fixture.session));
    // When a bounded receive is issued, then the caller expires independently of its blocked reader.
    assert!(transport
        .receive(Deadline::after(Duration::from_millis(10)).unwrap())
        .is_err());
    transport.close();
}
