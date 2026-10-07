use openpilot_can::Frame;
use openpilot_card::isotp::{rx_address, CanClient, CanIo, Error, IsoTpMessage};
use std::collections::VecDeque;

#[derive(Default)]
struct Io {
    received: VecDeque<Vec<Frame>>,
    sent: Vec<Frame>,
    delays: Vec<f64>,
    now: f64,
}
impl CanIo for Io {
    fn receive(&mut self) -> Result<Vec<Frame>, Error> {
        Ok(self.received.pop_front().unwrap_or_default())
    }
    fn send(&mut self, frame: Frame) -> Result<(), Error> {
        self.sent.push(frame);
        Ok(())
    }
    fn sleep(&mut self, seconds: f64) -> Result<(), Error> {
        self.delays.push(seconds);
        Ok(())
    }
    fn now(&mut self) -> f64 {
        self.now += 0.01;
        self.now
    }
}
fn message(single: bool, sub: Option<u8>) -> IsoTpMessage {
    IsoTpMessage::new(
        CanClient::new(0x700, Some(0x708), 0, sub, None),
        0.,
        single,
        0.01,
    )
    .unwrap()
}
fn frame(data: &[u8]) -> Frame {
    Frame {
        address: 0x708,
        data: data.to_vec(),
        bus: 0,
    }
}

#[test]
fn receive_reassembles_payload_and_emits_source_flow_control() {
    // Given a pending single-frame query and a segmented response.
    let mut state = message(false, None);
    let mut io = Io::default();
    state.send(&[0x22, 0xf1, 0x90], false, &mut io).unwrap();
    io.received.push_back(vec![
        frame(&[0x10, 9, 1, 2, 3, 4, 5, 6]),
        frame(&[0x21, 7, 8, 9, 0, 0, 0, 0]),
    ]);
    // When the actual transport frames are received.
    let response = state.recv(None, &mut io).unwrap();
    // Then the payload and outgoing flow-control bytes match the source.
    assert_eq!(response, (Some(vec![1, 2, 3, 4, 5, 6, 7, 8, 9]), false));
    assert_eq!(io.sent[1].data, [0x30, 0, 10, 0, 0, 0, 0, 0]);
}

#[test]
fn transmit_keeps_source_block_size_and_separation_delay() {
    // Given an ISO-TP request larger than one CAN frame.
    let mut state = message(false, None);
    let mut io = Io::default();
    state.send(&[1; 20], false, &mut io).unwrap();
    io.received.push_back(vec![frame(&[0x30, 0, 0xf1])]);
    // When the ECU permits the remaining frames.
    assert_eq!(state.recv(None, &mut io).unwrap(), (None, false));
    // Then both consecutive frames are sent with the inherited 0xf1 delay interpretation.
    assert_eq!(io.sent.len(), 3);
    assert_eq!(io.sent[1].data[0], 0x21);
    assert_eq!(io.sent[2].data[0], 0x22);
    assert!((io.delays[0] - 0.0113).abs() < 1e-12);
}

#[test]
fn incorrect_consecutive_index_retains_explicit_failure() {
    // Given a response whose first frame has been accepted.
    let mut state = message(false, None);
    let mut io = Io::default();
    state.send(&[1], false, &mut io).unwrap();
    io.received
        .push_back(vec![frame(&[0x10, 9, 1, 2, 3, 4, 5, 6])]);
    state.recv(None, &mut io).unwrap();
    io.received.push_back(vec![frame(&[0x22, 7, 8, 9])]);
    // When the ECU skips sequence index 1.
    let error = state.recv(None, &mut io).unwrap_err();
    // Then the transport rejects it instead of emitting fabricated data.
    assert!(matches!(error, Error::ConsecutiveIndex));
}

#[test]
fn address_mapping_preserves_standard_extended_and_functional_branches() {
    // Given the source address classes.
    let inputs = [
        (0x700, Some(0x708)),
        (0x18da10f1, Some(0x18daf110)),
        (0x7df, None),
    ];
    // When they are mapped to response addresses.
    let mapped: Vec<_> = inputs
        .iter()
        .map(|(tx, _)| rx_address(*tx, 8).unwrap())
        .collect();
    // Then physical offsets and functional wildcard handling are preserved.
    assert_eq!(mapped, inputs.iter().map(|(_, rx)| *rx).collect::<Vec<_>>());
}

#[test]
fn address_mapping_preserves_negative_chrysler_response_offset() {
    assert_eq!(rx_address(0x720, -640).unwrap(), Some(0x4a0));
    assert_eq!(rx_address(0x700, -0x701).unwrap(), Some(-1));
}
