use openpilot_control_tools::joystick::{Gamepad, Keyboard, Profile};

#[test]
fn keyboard_cancel_resets_on_unmapped_key_without_resetting_axes() {
    let mut owner = Keyboard::default();
    for _ in 0..25 {
        owner.update("W");
    }
    owner.update("C");
    assert!(owner.cancel);
    assert!(!owner.update("İ"));
    assert!(!owner.cancel);
    assert_eq!(owner.axes, [1.0, 0.0]);
    owner.update("r");
    assert_eq!(owner.axes, [0.0, 0.0]);
}

#[test]
fn gamepad_disconnect_keeps_cancel_and_calibration_for_recovery() {
    let mut owner = Gamepad::new(Profile::Pc);
    owner.update("ABS_RZ", 255).unwrap();
    owner.update("BTN_NORTH", 1).unwrap();
    let minimum = owner.minimum;
    owner.disconnected();
    assert_eq!(owner.axes, [0.0, 0.0]);
    assert_eq!(owner.minimum, minimum);
    assert!(owner.cancel);
    owner.update("ABS_Z", i32::MIN).unwrap();
    assert_eq!(owner.axes[0], 1.0);
}

#[cfg(feature = "native")]
#[test]
fn input_publication_keeps_source_buttons_empty() {
    use openpilot_cereal::log_capnp::event;
    let bytes = openpilot_control_tools::joystick_input::encode([0.05, -1.0], 123);
    let message =
        capnp::serialize::read_message(bytes.as_slice(), capnp::message::ReaderOptions::new())
            .unwrap();
    let root = message.get_root::<event::Reader<'_>>().unwrap();
    assert!(root.get_valid());
    let event::Which::TestJoystick(joystick) = root.which().unwrap() else {
        panic!("wrong event")
    };
    let joystick = joystick.unwrap();
    assert_eq!(
        joystick.get_axes().unwrap().iter().collect::<Vec<_>>(),
        [0.05_f32, -1.0]
    );
    assert!(joystick.get_buttons().unwrap().is_empty());
}
