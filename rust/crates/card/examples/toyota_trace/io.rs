use openpilot_can::Frame;
use openpilot_card::{firmware_query::StartupIo, isotp, query::QueryIo};

#[derive(Default)]
pub struct Io {
    pub calls: Vec<Vec<String>>,
    incoming: Vec<Vec<Frame>>,
    pub sent: Vec<Frame>,
    pub logs: Vec<Vec<String>>,
}
impl QueryIo for Io {
    fn receive(&mut self, _wait: bool) -> Result<Vec<Vec<Frame>>, isotp::Error> {
        self.calls.push(vec!["recv".into()]);
        Ok(std::mem::take(&mut self.incoming))
    }
    fn send(&mut self, frames: &[Frame]) -> Result<(), isotp::Error> {
        self.calls.push(vec!["send".into()]);
        self.sent.extend_from_slice(frames);
        for frame in frames {
            if frame.address == 0x750 {
                let data = match frame.data.as_slice() {
                    [0xf, 2, 0x10, 3, ..] => vec![0xf, 2, 0x50, 3, 0, 0, 0, 0],
                    [0xf, 3, 0x28, 1, 1, ..] => vec![0xf, 3, 0x68, 1, 1, 0, 0, 0],
                    _ => {
                        return Err(isotp::Error::Io(std::io::Error::other(
                            "unexpected Toyota lifecycle request",
                        )));
                    }
                };
                self.incoming.push(vec![Frame {
                    address: 0x758,
                    bus: 0,
                    data,
                }]);
            }
        }
        Ok(())
    }
    fn sleep(&mut self, _seconds: f64) -> Result<(), isotp::Error> {
        self.calls.push(vec!["sleep".into()]);
        Ok(())
    }
    fn now(&mut self) -> f64 {
        0.
    }
    fn log(&mut self, level: openpilot_card::query::DiagnosticLevel, message: &str) {
        self.logs.push(vec![
            format!("{level:?}").to_ascii_uppercase(),
            message.to_owned(),
        ]);
    }
}
impl StartupIo for Io {
    fn set_obd_multiplexing(&mut self, _enabled: bool) -> Result<(), isotp::Error> {
        self.calls.push(vec!["obd".into()]);
        Ok(())
    }
}
