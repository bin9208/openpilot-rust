use openpilot_cereal::log_capnp::event;
use openpilot_pandad::inputs::Inputs;

fn selfdrive(valid: bool, enabled: bool) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<event::Builder<'_>>();
    event.set_valid(valid);
    event.init_selfdrive_state().set_enabled(enabled);
    capnp::serialize::write_message_to_words(&message)
}

#[test]
fn selfdrive_eligibility_uses_validity_and_strict_cpp_receive_age() {
    let mut inputs = Inputs::new(false, 100.0);
    assert!(!inputs.engaged(0));
    inputs
        .update(1_000_000_000, &[selfdrive(true, true)])
        .unwrap();
    assert!(inputs.engaged(1_099_999_999));
    assert!(!inputs.engaged(1_100_000_000));
    inputs
        .update(1_100_000_000, &[selfdrive(false, true)])
        .unwrap();
    assert!(!inputs.engaged(1_100_000_000));
    inputs
        .update(1_100_000_001, &[selfdrive(true, false)])
        .unwrap();
    assert!(!inputs.engaged(1_100_000_001));
}

#[test]
fn simulation_retains_seen_valid_state_and_starts_without_engagement() {
    let mut inputs = Inputs::new(true, 100.0);
    assert!(!inputs.engaged(1_000_000_000));
    inputs.update(1, &[selfdrive(true, true)]).unwrap();
    inputs.update(100_000_000_000, &[]).unwrap();
    assert!(inputs.engaged(100_000_000_000));
}

#[test]
fn peripheral_inputs_keep_invalid_payloads_but_clear_updated_flags_each_tick() {
    let mut camera = capnp::message::Builder::new_default();
    let mut event = camera.init_root::<event::Builder<'_>>();
    event.set_valid(false);
    event.set_log_mono_time(123);
    let mut sample = event.init_driver_camera_state();
    sample.set_frame_id(7);
    sample.set_integ_lines(201);
    let camera = capnp::serialize::write_message_to_words(&camera);
    let mut fan = capnp::message::Builder::new_default();
    fan.init_root::<event::Builder<'_>>()
        .init_device_state()
        .set_fan_speed_percent_desired(51);
    let fan = capnp::serialize::write_message_to_words(&fan);
    let mut inputs = Inputs::new(false, 100.0);
    inputs.update(321, &[camera, fan]).unwrap();
    let input = inputs.peripheral(321, true);
    assert_eq!(input.frame, 1);
    assert_eq!(input.fan_speed, Some(51));
    let camera = input.camera.unwrap();
    assert_eq!(
        (
            camera.frame_id,
            camera.integration_lines,
            camera.mono_time_ns
        ),
        (7, 201, 123)
    );
    inputs.update(400, &[]).unwrap();
    let input = inputs.peripheral(400, false);
    assert_eq!(input.frame, 2);
    assert!(input.camera.is_none() && input.fan_speed.is_none() && !input.fan_control);
}
