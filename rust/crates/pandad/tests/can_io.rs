use openpilot_pandad::{
    can::{Encoder, Frame, RECEIVE_SIZE},
    can_io::{send_is_current, BulkTransport, CanIo, Outgoing},
    device::{Control, Transport},
};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    convert::Infallible,
};

#[derive(Default)]
struct Recorder {
    replies: RefCell<VecDeque<(Vec<u8>, i32, bool)>>,
    healthy: Cell<bool>,
    calls: RefCell<Vec<String>>,
}

impl Transport for Recorder {
    type Error = Infallible;
    fn control_read(&self, _: Control, _: &mut [u8]) -> Result<i32, Self::Error> {
        panic!("unexpected control read")
    }
    fn control_write(&self, command: Control) -> Result<i32, Self::Error> {
        assert_eq!(command, Control::new(0xc0, 0, 0));
        self.calls.borrow_mut().push("reset".into());
        Ok(0)
    }
}

impl BulkTransport for Recorder {
    fn bulk_read(
        &self,
        endpoint: u8,
        output: &mut [u8],
        timeout_ms: u32,
    ) -> Result<i32, Self::Error> {
        self.calls
            .borrow_mut()
            .push(format!("read:{endpoint}:{}:{timeout_ms}", output.len()));
        if endpoint == 0xab {
            return Ok(output.len() as i32);
        }
        let (bytes, count, healthy) = self.replies.borrow_mut().pop_front().unwrap();
        output[..bytes.len()].copy_from_slice(&bytes);
        self.healthy.set(healthy);
        Ok(count)
    }
    fn bulk_write(&self, endpoint: u8, input: &[u8], timeout_ms: u32) -> Result<i32, Self::Error> {
        self.calls
            .borrow_mut()
            .push(format!("write:{endpoint}:{}:{timeout_ms}", input.len()));
        Ok(-1)
    }
    fn comms_healthy(&self) -> bool {
        self.calls.borrow_mut().push("healthy".into());
        self.healthy.get()
    }
}

fn packet() -> Vec<u8> {
    let mut result = Vec::new();
    let mut write = |bytes: &[u8]| {
        result.extend_from_slice(bytes);
        Ok::<_, Infallible>(())
    };
    let mut encoder = Encoder::new(0);
    encoder.push(0x123, 2, &[7, 8], &mut write).unwrap();
    encoder.finish(&mut write).unwrap();
    result
}

#[test]
fn unhealthy_read_discards_new_bytes_but_retains_the_previous_partial_frame() {
    let bytes = packet();
    let transport = Recorder::default();
    transport.replies.borrow_mut().extend([
        (bytes[..4].to_vec(), 4, true),
        (vec![0xff; 4], 4, false),
        (bytes[4..].to_vec(), 4, true),
    ]);
    let mut io = CanIo::new(4, false);
    let mut frames = Vec::new();
    assert!(io
        .receive(&transport, &mut frames, || panic!("checksum"))
        .unwrap());
    assert!(!io
        .receive(&transport, &mut frames, || panic!("checksum"))
        .unwrap());
    assert_eq!(io.remaining(), &bytes[..4]);
    assert!(io
        .receive(&transport, &mut frames, || panic!("checksum"))
        .unwrap());
    assert_eq!(
        frames,
        [Frame {
            address: 0x123,
            src: 6,
            data: vec![7, 8]
        }]
    );
}

#[test]
fn checksum_failure_logs_before_reset_and_preserves_already_decoded_frames() {
    let mut bytes = packet();
    let mut invalid = packet();
    invalid[5] ^= 1;
    bytes.extend(invalid);
    let transport = Recorder::default();
    transport
        .replies
        .borrow_mut()
        .push_back((bytes.clone(), bytes.len() as i32, true));
    let mut io = CanIo::new(0, false);
    let mut frames = Vec::new();
    assert!(!io
        .receive(&transport, &mut frames, || transport
            .calls
            .borrow_mut()
            .push("checksum".into()))
        .unwrap());
    assert_eq!(frames.len(), 1);
    assert!(io.remaining().is_empty());
    assert_eq!(
        *transport.calls.borrow(),
        [
            format!("read:129:{RECEIVE_SIZE}:0"),
            "healthy".into(),
            "checksum".into(),
            "reset".into()
        ]
    );
}

#[test]
fn maxout_reads_only_remaining_bandwidth_after_a_healthy_receive() {
    let transport = Recorder::default();
    transport
        .replies
        .borrow_mut()
        .extend([(packet(), 8, true), (vec![], 0, false)]);
    let mut io = CanIo::new(0, true);
    let mut frames = Vec::new();
    assert!(io
        .receive(&transport, &mut frames, || panic!("checksum"))
        .unwrap());
    assert!(!io
        .receive(&transport, &mut frames, || panic!("checksum"))
        .unwrap());
    assert_eq!(
        *transport.calls.borrow(),
        [
            format!("read:129:{RECEIVE_SIZE}:0"),
            "healthy".into(),
            format!("read:171:{}:0", RECEIVE_SIZE - 8),
            format!("read:129:{RECEIVE_SIZE}:0"),
            "healthy".into(),
        ]
    );
}

#[test]
fn sending_keeps_source_chunking_and_continues_after_negative_write_result() {
    let transport = Recorder::default();
    let data = [0; 64];
    let frames = (0..9)
        .map(|_| Outgoing {
            address: 0x123,
            src: 4,
            data: &data,
        })
        .collect::<Vec<_>>();
    CanIo::new(4, false).send(&transport, &frames).unwrap();
    assert_eq!(
        *transport.calls.borrow(),
        ["write:3:280:5", "write:3:280:5", "write:3:70:5"]
    );
}

#[test]
fn send_queue_age_clamps_future_timestamps_and_rejects_exactly_one_second() {
    assert!(send_is_current(0, u64::MAX, false));
    assert!(send_is_current(1_000_000_000, 1, false));
    assert!(!send_is_current(1_000_000_000, 0, false));
    assert!(!send_is_current(1, 1, true));
}
