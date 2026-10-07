use openpilot_can::Frame;
use openpilot_card::{
    firmware_query::StartupIo,
    isotp,
    query::{DiagnosticLevel, QueryIo},
};
#[derive(Default)]
pub struct Io {
    pub calls: Vec<String>,
}
impl QueryIo for Io {
    fn receive(&mut self, _wait: bool) -> Result<Vec<Vec<Frame>>, isotp::Error> {
        self.calls.push("receive".into());
        Ok(Vec::new())
    }
    fn send(&mut self, _frames: &[Frame]) -> Result<(), isotp::Error> {
        self.calls.push("send".into());
        Ok(())
    }
    fn sleep(&mut self, _seconds: f64) -> Result<(), isotp::Error> {
        self.calls.push("sleep".into());
        Ok(())
    }
    fn now(&mut self) -> f64 {
        self.calls.push("now".into());
        0.
    }
    fn log(&mut self, _level: DiagnosticLevel, _message: &str) {
        self.calls.push("log".into());
    }
}
impl StartupIo for Io {
    fn set_obd_multiplexing(&mut self, _enabled: bool) -> Result<(), isotp::Error> {
        self.calls.push("obd".into());
        Ok(())
    }
}
