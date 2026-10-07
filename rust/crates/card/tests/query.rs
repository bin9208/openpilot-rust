use openpilot_can::Frame;
use openpilot_card::isotp::Error;
use openpilot_card::query::{ParallelQuery, QueryConfig, QueryIo, Target};
use std::collections::VecDeque;

#[derive(Default)]
struct Io {
    batches: VecDeque<Vec<Vec<Frame>>>,
    sent: Vec<Frame>,
    now: f64,
}
impl QueryIo for Io {
    fn receive(&mut self, _wait_for_one: bool) -> Result<Vec<Vec<Frame>>, Error> {
        Ok(self.batches.pop_front().unwrap_or_default())
    }
    fn send(&mut self, frames: &[Frame]) -> Result<(), Error> {
        self.sent.extend_from_slice(frames);
        Ok(())
    }
    fn sleep(&mut self, seconds: f64) -> Result<(), Error> {
        self.now += seconds;
        Ok(())
    }
    fn now(&mut self) -> f64 {
        self.now += 0.001;
        self.now
    }
}
fn response(data: &[u8]) -> Vec<Vec<Frame>> {
    vec![vec![Frame {
        address: 0x708,
        data: data.to_vec(),
        bus: 0,
    }]]
}

#[test]
fn sequential_requests_return_only_final_response_payload() {
    // Given an extended diagnostic session followed by a firmware read.
    let mut query = ParallelQuery::new(QueryConfig {
        bus: 0,
        targets: &[Target::new(0x700, None)],
        request: &[vec![0x10, 3], vec![0x22, 0xf1, 0x90]],
        response: &[vec![0x50, 3], vec![0x62, 0xf1, 0x90]],
        response_offset: 8,
        functional_addrs: &[],
        response_pending_timeout: 10.,
    })
    .unwrap();
    let mut io = Io {
        batches: VecDeque::from([
            vec![],
            response(&[2, 0x50, 3]),
            response(&[4, 0x62, 0xf1, 0x90, 0x55]),
        ]),
        ..Io::default()
    };
    // When the source query sequence runs over the adapter.
    let output = query.get_data(0.1, 60., &mut io).unwrap();
    // Then it strips the expected response prefix and sends the exact second request.
    assert_eq!(output, vec![(Target::new(0x700, None), vec![0x55])]);
    assert_eq!(io.sent[1].data, [3, 0x22, 0xf1, 0x90, 0, 0, 0, 0]);
}

#[test]
fn pending_response_extends_timeout_without_resending_request() {
    // Given an ECU response-pending message followed by a delayed positive response.
    let mut query = ParallelQuery::new(QueryConfig {
        bus: 0,
        targets: &[Target::new(0x700, None)],
        request: &[vec![0x22]],
        response: &[vec![0x62]],
        response_offset: 8,
        functional_addrs: &[],
        response_pending_timeout: 1.,
    })
    .unwrap();
    let mut io = Io {
        batches: VecDeque::from([
            vec![],
            response(&[3, 0x7f, 0x22, 0x78]),
            vec![],
            vec![],
            response(&[2, 0x62, 0x55]),
        ]),
        ..Io::default()
    };
    // When the pending response crosses the initial timeout.
    let output = query.get_data(0.003, 60., &mut io).unwrap();
    // Then the final result is accepted with no duplicate diagnostic request.
    assert_eq!(output, vec![(Target::new(0x700, None), vec![0x55])]);
    assert_eq!(io.sent.len(), 1);
}

#[test]
fn expired_query_returns_no_invented_firmware() {
    // Given an ECU that never responds.
    let mut query = ParallelQuery::new(QueryConfig {
        bus: 0,
        targets: &[Target::new(0x700, None)],
        request: &[vec![0x22]],
        response: &[vec![0x62]],
        response_offset: 8,
        functional_addrs: &[],
        response_pending_timeout: 10.,
    })
    .unwrap();
    let mut io = Io::default();
    // When its timeout expires.
    let output = query.get_data(0.003, 60., &mut io).unwrap();
    // Then no successful firmware result is produced.
    assert!(output.is_empty());
}
