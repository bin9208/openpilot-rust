use openpilot_crash_reporting::{Configuration, Error as SentryError, NativeException, Sdk};
use openpilot_hardware_control::{AmplifierAction, Command, CommandOutput, Error, Platform};
use openpilot_runtime_version::JsonValue;

#[derive(Default)]
pub struct Capture {
    pub messages: Vec<String>,
}
impl Sdk for Capture {
    fn init(&mut self, _: Configuration) -> Result<(), SentryError> {
        Ok(())
    }
    fn set_user(&mut self, _: Option<String>) -> Result<(), SentryError> {
        Ok(())
    }
    fn set_tag(&mut self, _: &str, _: &JsonValue) -> Result<(), SentryError> {
        Ok(())
    }
    fn set_extra(&mut self, _: &str, _: &JsonValue) -> Result<(), SentryError> {
        Ok(())
    }
    fn capture_message(&mut self, _: &str) -> Result<(), SentryError> {
        panic!("unexpected message")
    }
    fn capture_exception(&mut self, exception: &NativeException) -> Result<(), SentryError> {
        self.messages.push(exception.message.clone());
        Ok(())
    }
    fn flush(&mut self) -> Result<(), SentryError> {
        self.messages.push("flush".into());
        Ok(())
    }
}
#[derive(Default)]
pub struct Board {
    pub trace: Vec<String>,
}
impl Platform for Board {
    fn read(&mut self, _: &str) -> Result<String, Error> {
        panic!("unexpected board read")
    }
    fn write(&mut self, _: &str, _: &str) -> Result<(), Error> {
        panic!("unexpected board write")
    }
    fn command(&mut self, command: &Command) -> Result<CommandOutput, Error> {
        self.trace.push(format!("{command:?}"));
        Ok(CommandOutput {
            status: 0,
            stdout: vec![],
        })
    }
    fn print(&mut self, _: &str) -> Result<(), Error> {
        panic!("unexpected PC action")
    }
    fn sleep(&mut self, _: f64) -> Result<(), Error> {
        panic!("unexpected sleep")
    }
    fn monotonic(&mut self) -> Result<f64, Error> {
        panic!("unexpected clock")
    }
    fn touch(&mut self, path: &str) -> Result<(), Error> {
        self.trace.push(format!("touch {path}"));
        Ok(())
    }
    fn sync(&mut self) -> Result<(), Error> {
        self.trace.push("sync".into());
        Ok(())
    }
    fn c3x_lite(&mut self) -> Result<bool, Error> {
        panic!("unexpected Params")
    }
    fn amplifier(&mut self, _: AmplifierAction<'_>) -> Result<bool, Error> {
        panic!("unexpected amplifier")
    }
}

pub struct Identity;
impl openpilot_hardware_info::HardwareInfo for Identity {
    fn get_device_type(&self) -> Result<String, openpilot_hardware_info::Error> {
        Ok("fixture".into())
    }
    fn get_serial(&self) -> Result<String, openpilot_hardware_info::Error> {
        Ok("fixture-serial".into())
    }
}
