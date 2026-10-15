use openpilot_jetlink::phase::{Gate, Publisher, State, CAMERA_SKEW_NS, FRESH_NS, WAIT};
use std::{
    thread,
    time::{Duration, Instant},
};
fn packet(sof: u64, sent: u64) -> Vec<u8> {
    [sof.to_le_bytes(), sent.to_le_bytes()].concat()
}
#[test]
fn phase_admission_preserves_strict_freshness_monotonicity_and_skew() {
    let mut state = State::default();
    let now = 1_000_000_000;
    for (sof, sent, accepted) in [
        (0, now, false),
        (now + 1, now, false),
        (now, now + 1, false),
        (1, now - FRESH_NS, false),
        (1, now - FRESH_NS + 1, true),
        (2, now, true),
        (3, now, false),
    ] {
        let previous = state.sent_at;
        state.accept(&packet(sof, sent), now);
        assert_eq!(state.sent_at != previous, accepted);
    }
    assert!(!state.fresh(now + FRESH_NS));
    assert!(!state.behind(state.source_sof + CAMERA_SKEW_NS));
    assert!(state.behind(state.source_sof + CAMERA_SKEW_NS + 1));
    for length in [0, 15, 17] {
        state.accept(&vec![0; length], now);
    }
    assert_eq!(state.sent_at, now);
}
#[test]
fn real_datagram_wait_releases_on_current_upload_and_stays_bounded() {
    let name = format!("jetlink-phase-test-{}", std::process::id());
    let mut gate = Gate::bind(name.as_bytes());
    let publisher = Publisher::new(name.as_bytes());
    let sof = openpilot_jetlink::now_ns();
    publisher.sent(sof);
    gate.wait(sof);
    assert_eq!(gate.state.source_sof, sof);
    let worker = thread::spawn(move || {
        thread::sleep(Duration::from_millis(5));
        publisher.sent(sof + CAMERA_SKEW_NS + 1);
    });
    gate.wait(sof + CAMERA_SKEW_NS + 1);
    worker.join().unwrap();
    assert_eq!(gate.state.source_sof, sof + CAMERA_SKEW_NS + 1);
    let start = Instant::now();
    gate.wait(sof + 2 * CAMERA_SKEW_NS + 2);
    assert!(start.elapsed() >= WAIT);
    assert!(start.elapsed() < Duration::from_millis(250));
    thread::sleep(Duration::from_millis(76));
    let start = Instant::now();
    gate.wait(sof + 100_000_000);
    assert!(start.elapsed() < Duration::from_millis(20));
}

#[test]
fn original_phase_accept_matches512_source_packets() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("phase_source_vectors.json")).unwrap();
    assert_eq!(
        fixture["source_commit"],
        "e8e152dbcfd9574a1791ed12026d1880e87689b3"
    );
    assert_eq!(
        fixture["source_sha256"],
        "66b46f0bc022558e7864888e8bd1ddc69bdfb6a4cbac96c9f42b88ba339d3ba2"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 512);
    let mut state = State::default();
    for case in cases {
        let packet: Vec<u8> = serde_json::from_value(case["packet"].clone()).unwrap();
        state.accept(&packet, case["now"].as_u64().unwrap());
        assert_eq!(state.source_sof, case["source_sof"].as_u64().unwrap());
        assert_eq!(state.sent_at, case["sent_at"].as_u64().unwrap());
    }
}

#[test]
fn local_rpc_sof_extension_reaches_actual_owner_without_changing_legacy_request() {
    use openpilot_jetlink::{
        contract,
        owner::{self, Backend},
        rpc, Deadline, Error,
    };
    struct Capture(u64);
    impl Backend for Capture {
        fn source_sof(&mut self, sof: u64) {
            self.0 = sof;
        }
        fn connect(&mut self) -> Result<contract::Identity, Error> {
            Ok(contract::Identity::new())
        }
        fn infer(
            &mut self,
            _: u32,
            _: &[u8],
            _: &[f32],
            _: Deadline,
            _: bool,
        ) -> Result<Vec<f32>, Error> {
            Ok(vec![0.; contract::OUTPUT_FLOATS])
        }
        fn dead(&self) -> bool {
            false
        }
        fn close(&mut self) {}
    }
    let mut backend = Capture(99);
    let generation = [17; 16];
    let image = vec![0; contract::WARPED_BYTES];
    let mut request = rpc::encode_request(
        generation,
        42,
        Deadline::after(Duration::from_millis(50)).unwrap(),
        true,
        &image,
        &[0.; 12],
    );
    owner::infer_request(&mut backend, generation, &request).unwrap();
    assert_eq!(backend.0, 0);
    request[21..29].copy_from_slice(
        &Deadline::after(Duration::from_millis(50))
            .unwrap()
            .0
            .to_le_bytes(),
    );
    request.extend(123_456_789u64.to_le_bytes());
    owner::infer_request(&mut backend, generation, &request).unwrap();
    assert_eq!(backend.0, 123_456_789);
    request.push(0);
    assert!(owner::infer_request(&mut backend, generation, &request).is_err());
}
