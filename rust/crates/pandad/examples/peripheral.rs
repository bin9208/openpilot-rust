use openpilot_pandad::peripheral::{Input, Output, Peripheral};
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead};

#[derive(Deserialize)]
struct Step {
    #[serde(flatten)]
    input: Input,
    #[serde(default)]
    driver_view: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Command {
    DriverView(bool),
    Fan(u16),
    PandaIr(u16),
    HardwareIr(i32),
}

#[derive(Default)]
struct Recorder {
    driver_view: bool,
    commands: Vec<Command>,
}

impl Output for Recorder {
    fn driver_view_enabled(&mut self) -> bool {
        self.commands.push(Command::DriverView(self.driver_view));
        self.driver_view
    }
    fn set_fan_speed(&mut self, speed: u16) {
        self.commands.push(Command::Fan(speed));
    }
    fn set_panda_ir_power(&mut self, power: u16) {
        self.commands.push(Command::PandaIr(power));
    }
    fn set_hardware_ir_power(&mut self, power: i32) {
        self.commands.push(Command::HardwareIr(power));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let steps: Vec<Step> = serde_json::from_str(&line?)?;
        let mut peripheral = Peripheral::default();
        let mut output = Recorder::default();
        let mut trace = Vec::with_capacity(steps.len());
        for step in steps {
            output.driver_view = step.driver_view;
            peripheral.update(step.input, &mut output)?;
            trace.push(std::mem::take(&mut output.commands));
        }
        println!("{}", serde_json::to_string(&trace)?);
    }
    Ok(())
}
