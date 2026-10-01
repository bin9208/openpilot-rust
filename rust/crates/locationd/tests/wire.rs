use openpilot_cereal::log_capnp::event;
use openpilot_locationd::{types::Input, wire};

#[test]
fn filter_timestamp_preserves_uninitialized_and_range_behavior() {
    use openpilot_locationd::types::filter_timestamp;
    assert_eq!(filter_timestamp(f64::NAN).ok(), Some(0));
    assert_eq!(filter_timestamp(1.25).ok(), Some(1_250_000_000));
    assert!(filter_timestamp(-0.1).is_err());
    assert!(filter_timestamp(f64::INFINITY).is_err());
    assert!(filter_timestamp(u64::MAX as f64).is_err());
}

#[test]
fn invalid_sensor_payload_is_not_interpreted() -> Result<(), Box<dyn std::error::Error>> {
    let mut message = capnp::message::Builder::new_default();
    let mut root = message.init_root::<event::Builder<'_>>();
    root.set_valid(false);
    root.init_accelerometer().init_acceleration().init_v(0);
    let event = wire::decode(&capnp::serialize::write_message_to_words(&message))?;
    assert!(!event.valid);
    assert!(matches!(event.input, Input::Ignored));
    Ok(())
}

#[test]
fn initial_covariance_uses_stored_std_directly() -> Result<(), Box<dyn std::error::Error>> {
    let mut message = capnp::message::Builder::new_default();
    let root = message.init_root::<event::Builder<'_>>();
    let mut state = root.init_live_pose().init_debug_filter_state();
    let mut std = state.reborrow().init_std(18);
    for i in 0..18 {
        std.set(i, 2.0 + f64::from(i) * 0.25);
    }
    let seed = wire::seed(&capnp::serialize::write_message_to_words(&message))?;
    assert_eq!(seed.covariance[0], 2.0);
    assert_eq!(seed.covariance[19], 2.25);
    assert_eq!(seed.covariance[1], 0.);
    Ok(())
}

#[test]
fn malformed_initial_dimensions_fail_before_native_boundary(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut message = capnp::message::Builder::new_default();
    message
        .init_root::<event::Builder<'_>>()
        .init_live_pose()
        .init_debug_filter_state()
        .init_value(17);
    assert!(wire::seed(&capnp::serialize::write_message_to_words(&message)).is_err());
    Ok(())
}
