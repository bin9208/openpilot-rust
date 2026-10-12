use openpilot_cluster::simulator::gamepad::DualSenseSimulator;
use openpilot_sdl2::{Context, DeviceKind};
use std::path::Path;

#[test]
fn virtual_devices_drive_commands_and_release_context_after_close() {
    let context = Context::load(Path::new("libSDL2-2.0.so.0")).unwrap();
    let before = context.device_count();
    let joystick = context
        .attach_virtual(DeviceKind::Joystick, 6, 15, 1)
        .unwrap();
    let controller = context
        .attach_virtual(DeviceKind::Controller, 6, 15, 0)
        .unwrap();
    let mut generic =
        DualSenseSimulator::open_with_context(&context, joystick.device_index().unwrap(), 10.0)
            .unwrap();
    let mut mapped =
        DualSenseSimulator::open_with_context(&context, controller.device_index().unwrap(), 10.0)
            .unwrap();
    joystick.set_axis(0, 32767).unwrap();
    joystick.set_axis(2, -32768).unwrap();
    joystick.set_hat(0, 8).unwrap();
    controller.set_axis(0, 32767).unwrap();
    controller.set_button(10, true).unwrap();
    assert_eq!(
        generic.read_input(10.59).unwrap().steering.to_bits(),
        0.0_f64.to_bits()
    );
    assert_eq!(
        mapped.read_input(10.59).unwrap().steering.to_bits(),
        0.0_f64.to_bits()
    );
    let generic_input = generic.read_input(10.6).unwrap();
    let mapped_input = mapped.read_input(10.6).unwrap();
    assert_eq!(
        generic_input.steering.to_bits(),
        (32767.0_f64 / 32768.0).to_bits()
    );
    assert_eq!(
        generic_input.surround_yaw_deg.to_bits(),
        (-180.0_f64).to_bits()
    );
    assert!(generic_input.left_signal_requested);
    assert_eq!(mapped_input.steering.to_bits(), 1.0_f64.to_bits());
    assert!(mapped_input.right_signal_requested);
    assert!(generic.status_text().ends_with("SIG=L-"));
    assert!(mapped.status_text().ends_with("SIG=-R"));
    generic.close();
    generic.close();
    assert!(generic.read_input(11.0).is_err());
    drop(mapped);
    drop(joystick);
    drop(controller);
    assert_eq!(context.device_count(), before);
}
