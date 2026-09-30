use openpilot_hardware_control::{
    AmplifierAction, Command, CommandOutput, Error, HardwareControl, Platform,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{self, BufRead},
};

#[derive(Deserialize)]
struct Input {
    model: String,
    operations: Vec<Operation>,
    files: BTreeMap<String, String>,
    #[serde(default)]
    faults: BTreeMap<usize, i32>,
    #[serde(default)]
    lite: bool,
    #[serde(default)]
    command_status: i32,
    #[serde(default)]
    command_text: String,
    #[serde(default)]
    now: f64,
    #[serde(default = "enabled")]
    amplifier_result: bool,
}
fn enabled() -> bool {
    true
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Operation {
    Initialize,
    PowerSave { enabled: bool },
    Display { on: bool },
    Brightness { percent: f64 },
    Ir { percent: i32 },
    Reset,
    Recover,
    Booted,
    Reboot,
    Shutdown,
    Uninstall,
    HasPanda,
    Affine { core: u8, action: String },
    SudoWrite { path: String, value: String },
}
struct Fixture {
    input: Input,
    events: Vec<Value>,
}
impl Fixture {
    fn record(&mut self, event: Value) -> Result<(), Error> {
        let index = self.events.len();
        let path = event
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("fixture")
            .to_owned();
        self.events.push(event);
        match self.input.faults.get(&index) {
            Some(errno) => Err(Error::io(io::Error::from_raw_os_error(*errno), &path)),
            None => Ok(()),
        }
    }
}
impl Platform for Fixture {
    fn read(&mut self, path: &str) -> Result<String, Error> {
        self.record(json!({"kind":"read","path":path}))?;
        self.input
            .files
            .get(path)
            .cloned()
            .ok_or_else(|| Error::io(io::Error::from_raw_os_error(2), path))
    }
    fn write(&mut self, path: &str, value: &str) -> Result<(), Error> {
        self.record(json!({"kind":"write","path":path,"value":value}))?;
        self.input.files.insert(path.into(), value.into());
        Ok(())
    }
    fn command(&mut self, command: &Command) -> Result<CommandOutput, Error> {
        let event = match command {
            Command::Shell(text) => json!({"kind":"shell","text":text}),
            Command::Call(argv) => json!({"kind":"call","argv":argv}),
            Command::Output(argv) => json!({"kind":"output","argv":argv}),
        };
        self.record(event)?;
        Ok(CommandOutput {
            status: self.input.command_status,
            stdout: self.input.command_text.as_bytes().into(),
        })
    }
    fn print(&mut self, value: &str) -> Result<(), Error> {
        self.record(json!({"kind":"print","value":value}))
    }
    fn sleep(&mut self, seconds: f64) -> Result<(), Error> {
        self.record(json!({"kind":"sleep","seconds":seconds}))
    }
    fn monotonic(&mut self) -> Result<f64, Error> {
        self.record(json!({"kind":"monotonic"}))?;
        Ok(self.input.now)
    }
    fn touch(&mut self, path: &str) -> Result<(), Error> {
        self.record(json!({"kind":"touch","path":path}))?;
        self.input.files.entry(path.into()).or_default();
        Ok(())
    }
    fn sync(&mut self) -> Result<(), Error> {
        self.record(json!({"kind":"sync"}))
    }
    fn c3x_lite(&mut self) -> Result<bool, Error> {
        self.record(json!({"kind":"lite"}))?;
        Ok(self.input.lite)
    }
    fn amplifier(&mut self, action: AmplifierAction<'_>) -> Result<bool, Error> {
        let event = match action {
            AmplifierAction::Shutdown(disabled) => {
                json!({"kind":"amp_shutdown","disabled":disabled})
            }
            AmplifierAction::Initialize(model) => json!({"kind":"amp_initialize","model":model}),
        };
        self.record(event)?;
        Ok(self.input.amplifier_result)
    }
}
fn run(mut input: Input) -> Value {
    let mut hardware = if input.model == "pc" {
        HardwareControl::pc()
    } else {
        HardwareControl::board(&input.model)
    };
    let operations = std::mem::take(&mut input.operations);
    let mut platform = Fixture {
        input,
        events: Vec::new(),
    };
    let mut outcomes = Vec::new();
    for operation in operations {
        let result = match operation {
            Operation::Initialize => hardware
                .initialize_hardware(&mut platform)
                .map(|()| Value::Null),
            Operation::PowerSave { enabled } => hardware
                .set_power_save(&mut platform, enabled)
                .map(|()| Value::Null),
            Operation::Display { on } => {
                hardware.set_display_power(&mut platform, on);
                Ok(Value::Null)
            }
            Operation::Brightness { percent } => {
                hardware.set_screen_brightness(&mut platform, percent);
                Ok(Value::Null)
            }
            Operation::Ir { percent } => hardware
                .set_ir_power(&mut platform, percent)
                .map(|()| Value::Null),
            Operation::Reset => hardware
                .reset_internal_panda(&mut platform)
                .map(|()| Value::Null),
            Operation::Recover => hardware
                .recover_internal_panda(&mut platform)
                .map(|()| Value::Null),
            Operation::Booted => hardware.booted(&mut platform).map(Value::Bool),
            Operation::Reboot => hardware.reboot(&mut platform).map(|()| Value::Null),
            Operation::Shutdown => hardware.shutdown(&mut platform).map(|()| Value::Null),
            Operation::Uninstall => hardware.uninstall(&mut platform).map(|()| Value::Null),
            Operation::HasPanda => Ok(Value::Bool(hardware.has_internal_panda())),
            Operation::Affine { core, action } => hardware
                .affine_irq(&mut platform, core, &action)
                .map(|()| Value::Null),
            Operation::SudoWrite { path, value } => {
                openpilot_hardware_control::sudo_write(&mut platform, &path, &value)
                    .map(|()| Value::Null)
            }
        };
        outcomes.push(match result {
            Ok(value) => json!({"value":value}),
            Err(Error::Io { source, .. }) => json!({"error":"io","errno":source.raw_os_error()}),
            Err(Error::Command { status, .. }) => json!({"error":"command","status":status}),
            Err(Error::Other(_)) => json!({"error":"other"}),
        });
    }
    json!({"events":platform.events,"outcomes":outcomes,"files":platform.input.files})
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        println!("{}", run(serde_json::from_str(&line?)?));
    }
    Ok(())
}
