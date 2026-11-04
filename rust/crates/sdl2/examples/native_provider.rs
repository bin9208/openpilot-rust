//! Actual owned virtual devices against an explicitly selected runtime provider.
use openpilot_sdl2::{Context, DeviceKind};
use std::{env, fs, io, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err(io::Error::other("native_provider SDL_LIBRARY MAPS_OUTPUT").into());
    }
    let context = Context::load(&PathBuf::from(&args[0]))?;
    let before = context.device_count();
    for _ in 0..20 {
        let joystick = context.attach_virtual(DeviceKind::Joystick, 6, 15, 1)?;
        let controller = context.attach_virtual(DeviceKind::Controller, 6, 15, 0)?;
        assert_eq!(context.device_count(), before + 2);
        joystick.set_axis(0, -32768)?;
        joystick.set_axis(5, 32767)?;
        joystick.set_button(4, true)?;
        joystick.set_hat(0, 8)?;
        controller.set_axis(0, 32767)?;
        controller.set_button(9, true)?;
        let mut generic = context.open(joystick.device_index()?)?;
        let mut mapped = context.open(controller.device_index()?)?;
        let generic_sample = generic.snapshot()?;
        let mapped_sample = mapped.snapshot()?;
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
    fs::write(&args[1], fs::read("/proc/self/maps")?)?;
    println!("20 owned virtual controller/joystick cycles passed");
    Ok(())
}
