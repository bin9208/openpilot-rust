use super::DebugOmissionState;
use openpilot_torqued::{
    estimator::{Car, Estimator, Identity},
    random::RandomState,
    wire, Error, Fit,
};

struct Fixed(f64);
impl Fit for Fixed {
    fn estimate(&mut self, _: &[[f64; 3]]) -> Result<[f64; 3], Error> {
        Ok([self.0, 0., 0.12])
    }
}
fn fill(estimator: &mut Estimator) {
    for x in [-0.375, -0.25, -0.125, -0.0625, 0.0625, 0.125, 0.25, 0.375] {
        for _ in 0..510 {
            estimator.buckets.add(x, 2. * x);
        }
    }
}
fn publish(
    state: &mut DebugOmissionState,
    estimator: &mut Estimator,
    fit: &mut Fixed,
    debug: bool,
) -> bool {
    let packet = estimator.message(fit, debug).unwrap();
    let (bytes, omission) = wire::encode_publication(packet, 1, true).unwrap();
    let decoded = capnp::serialize::read_message(
        std::io::Cursor::new(bytes),
        capnp::message::ReaderOptions::new(),
    )
    .unwrap();
    let openpilot_cereal::log_capnp::event::LiveTorqueParameters(message) = decoded
        .get_root::<openpilot_cereal::log_capnp::event::Reader>()
        .unwrap()
        .which()
        .unwrap()
    else {
        panic!("liveTorqueParameters")
    };
    let message = message.unwrap();
    assert_eq!(
        message.get_total_bucket_points(),
        estimator.buckets.len() as f32
    );
    assert_eq!(message.get_points().unwrap().len(), 0);
    state.update(omission).is_some()
}

#[test]
fn omission_warning_rearms_after_real_fit_reset_and_debug_disable() {
    let mut estimator = Estimator::new(
        Car {
            identity: Identity {
                fingerprint: "test".into(),
                tuning: 3,
                torque: Some([0.12, 2.]),
            },
            allowed_brand: true,
        },
        (false, false),
        RandomState::seeded(42),
    );
    let mut state = DebugOmissionState::default();
    fill(&mut estimator);
    let mut fit = Fixed(2.);
    for expected in [true, false, false, false, false, false] {
        assert_eq!(
            publish(&mut state, &mut estimator, &mut fit, true),
            expected
        );
        assert_eq!(estimator.buckets.len(), 4080);
    }
    // IPC omission never removes diagnostic points from Params serialization.
    let packet = estimator.message(&mut fit, true).unwrap();
    let persisted = wire::encode(&packet, 1, true).unwrap();
    let reader = capnp::serialize::read_message(
        std::io::Cursor::new(persisted),
        capnp::message::ReaderOptions::new(),
    )
    .unwrap();
    let openpilot_cereal::log_capnp::event::LiveTorqueParameters(message) = reader
        .get_root::<openpilot_cereal::log_capnp::event::Reader>()
        .unwrap()
        .which()
        .unwrap()
    else {
        panic!("liveTorqueParameters")
    };
    assert_eq!(message.unwrap().get_points().unwrap().len(), 4080);

    // An invalid fit resets the actual estimator; its next fitting publication clears the warning state.
    assert!(!publish(
        &mut state,
        &mut estimator,
        &mut Fixed(f64::NAN),
        true
    ));
    assert_eq!(estimator.buckets.len(), 0);
    fill(&mut estimator);
    assert!(publish(&mut state, &mut estimator, &mut fit, true));
    assert!(!publish(&mut state, &mut estimator, &mut fit, true));
    // A no-points publication also clears the local state without changing the learned data.
    assert!(!publish(&mut state, &mut estimator, &mut fit, false));
    assert_eq!(estimator.buckets.len(), 4080);
    assert!(publish(&mut state, &mut estimator, &mut fit, true));
    assert!(!publish(&mut state, &mut estimator, &mut fit, true));
}
