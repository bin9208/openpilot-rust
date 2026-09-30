use openpilot_hardware_control::{
    Command, CommandOutput, Commands, Error, HardwareControl, LinuxPlatform, Platform,
};
use std::fs;

struct NoCommands;
impl Commands for NoCommands {
    fn run(&mut self, _command: &Command) -> Result<CommandOutput, Error> {
        panic!("filesystem test must not launch commands");
    }
}

#[test]
fn native_setters_write_source_values_in_temporary_tree() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    for path in [
        "sys/class/backlight/panel0-backlight",
        "sys/class/leds/led:switch_2",
        "sys/class/leds/led:torch_2",
        "sys/class/gpio/gpio124",
        "sys/class/gpio/gpio134",
        "data",
    ] {
        fs::create_dir_all(root.join(path)).unwrap();
    }
    fs::write(
        root.join("sys/class/backlight/panel0-backlight/max_brightness"),
        "4095\n",
    )
    .unwrap();
    let mut platform = LinuxPlatform::new(root, NoCommands);
    let hardware = HardwareControl::board("tici");
    hardware.set_display_power(&mut platform, false);
    hardware.set_screen_brightness(&mut platform, 43.7);
    hardware.set_ir_power(&mut platform, 37).unwrap();
    hardware.recover_internal_panda(&mut platform).unwrap();
    platform.touch("/data/__system_reset__").unwrap();
    for (path, expected) in [
        ("sys/class/backlight/panel0-backlight/bl_power", "4"),
        ("sys/class/backlight/panel0-backlight/brightness", "1789"),
        ("sys/class/leds/led:switch_2/brightness", "111\n"),
        ("sys/class/leds/led:torch_2/brightness", "111\n"),
        ("sys/class/gpio/gpio124/direction", "out"),
        ("sys/class/gpio/gpio134/direction", "out"),
        ("sys/class/gpio/gpio124/value", "0"),
        ("sys/class/gpio/gpio134/value", "0"),
        ("data/__system_reset__", ""),
    ] {
        assert_eq!(
            fs::read_to_string(root.join(path)).unwrap(),
            expected,
            "{path}"
        );
    }
}
