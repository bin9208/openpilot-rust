use openpilot_sdl2::{Context, DeviceKind};
use std::path::Path;

#[test]
fn owned_virtual_controller_and_joystick_release_without_affecting_other_devices() {
    let context = Context::load(Path::new("libSDL2-2.0.so.0")).unwrap();
    let before = context.device_count();
    let joystick = context
        .attach_virtual(DeviceKind::Joystick, 6, 15, 1)
        .unwrap();
    let controller = context
        .attach_virtual(DeviceKind::Controller, 6, 15, 0)
        .unwrap();
    assert_eq!(context.device_count(), before + 2);
    joystick.set_axis(0, -32768).unwrap();
    joystick.set_axis(5, 32767).unwrap();
    joystick.set_button(4, true).unwrap();
    joystick.set_hat(0, 8).unwrap();
    controller.set_axis(0, 32767).unwrap();
    controller.set_button(9, true).unwrap();
    let mut generic = context.open(joystick.device_index().unwrap()).unwrap();
    let mut mapped = context.open(controller.device_index().unwrap()).unwrap();
    let generic_sample = generic.snapshot().unwrap();
    let mapped_sample = mapped.snapshot().unwrap();
    assert_eq!(generic_sample.kind, DeviceKind::Joystick);
    assert_eq!(generic_sample.axes[0], -32768);
    assert_eq!(generic_sample.axes[5], 32767);
    assert!(generic_sample.buttons[4]);
    assert_eq!(generic_sample.hat, 8);
    assert_eq!(mapped_sample.kind, DeviceKind::Controller);
    assert_eq!(mapped_sample.axes[0], 32767);
    assert!(mapped_sample.buttons[9]);
    generic.close();
    generic.close();
    assert!(generic.snapshot().is_err());
    mapped.close();
    drop(joystick);
    drop(controller);
    assert_eq!(context.device_count(), before);
}
