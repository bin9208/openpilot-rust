use openpilot_pandad::peripheral::{CameraSample, Input, Output, Peripheral};

#[derive(Debug, PartialEq, Eq)]
enum Call {
    DriverView,
    Fan(u16),
    PandaIr(u16),
    HardwareIr(i32),
}

#[derive(Default)]
struct Recorder {
    driver_view: bool,
    calls: Vec<Call>,
}

impl Output for Recorder {
    fn driver_view_enabled(&mut self) -> bool {
        self.calls.push(Call::DriverView);
        self.driver_view
    }
    fn set_fan_speed(&mut self, speed: u16) {
        self.calls.push(Call::Fan(speed));
    }
    fn set_panda_ir_power(&mut self, power: u16) {
        self.calls.push(Call::PandaIr(power));
    }
    fn set_hardware_ir_power(&mut self, power: i32) {
        self.calls.push(Call::HardwareIr(power));
    }
}

#[test]
fn unchanged_commands_refresh_only_on_the_source_cadence() {
    let mut controller = Peripheral::default();
    let mut output = Recorder::default();
    let mut input = Input {
        frame: 1,
        fan_speed: Some(55),
        ..Input::default()
    };
    controller.update(input, &mut output).unwrap();
    assert_eq!(
        output.calls,
        [Call::Fan(55), Call::PandaIr(0), Call::HardwareIr(0)]
    );
    output.calls.clear();
    input.frame = 2;
    controller.update(input, &mut output).unwrap();
    assert!(output.calls.is_empty());
    input.frame = 100;
    controller.update(input, &mut output).unwrap();
    assert_eq!(
        output.calls,
        [Call::Fan(55), Call::PandaIr(0), Call::HardwareIr(0)]
    );
}

#[test]
fn camera_timeout_retains_power_at_exact_boundary_then_turns_it_off() {
    let mut controller = Peripheral::default();
    let mut output = Recorder {
        driver_view: true,
        ..Recorder::default()
    };
    let mut input = Input {
        frame: 1,
        now_ns: 2_000_000_000,
        camera: Some(CameraSample {
            frame_id: 0,
            integration_lines: 102_000,
            mono_time_ns: 2_000_000_000,
        }),
        ..Input::default()
    };
    controller.update(input, &mut output).unwrap();
    assert_eq!(
        output.calls,
        [Call::DriverView, Call::PandaIr(50), Call::HardwareIr(100)]
    );
    output.calls.clear();
    input.camera = None;
    input.now_ns = 3_000_000_000;
    controller.update(input, &mut output).unwrap();
    assert!(output.calls.is_empty());
    input.now_ns += 1;
    controller.update(input, &mut output).unwrap();
    assert_eq!(output.calls, [Call::PandaIr(0), Call::HardwareIr(0)]);
}

#[test]
fn camera_restart_resets_filters_and_samples_driver_view_again() {
    let mut controller = Peripheral::default();
    let mut output = Recorder {
        driver_view: true,
        ..Recorder::default()
    };
    let mut input = Input {
        frame: 1,
        camera: Some(CameraSample {
            frame_id: 10,
            integration_lines: 102_000,
            mono_time_ns: 0,
        }),
        ..Input::default()
    };
    controller.update(input, &mut output).unwrap();
    output.calls.clear();
    output.driver_view = false;
    input.camera.as_mut().unwrap().frame_id = 11;
    controller.update(input, &mut output).unwrap();
    assert!(output.calls.is_empty());
    input.camera.as_mut().unwrap().frame_id = 0;
    controller.update(input, &mut output).unwrap();
    assert_eq!(
        output.calls,
        [Call::DriverView, Call::PandaIr(0), Call::HardwareIr(0)]
    );
}

#[test]
fn disabled_fan_control_does_not_consume_the_pending_fan_setting() {
    let mut controller = Peripheral::default();
    let mut output = Recorder::default();
    let mut input = Input {
        frame: 1,
        fan_speed: Some(31),
        fan_control: false,
        ..Input::default()
    };
    controller.update(input, &mut output).unwrap();
    output.calls.clear();
    input.fan_control = true;
    controller.update(input, &mut output).unwrap();
    assert_eq!(output.calls, [Call::Fan(31)]);
}
