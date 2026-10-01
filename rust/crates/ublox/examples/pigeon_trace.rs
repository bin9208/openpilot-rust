use openpilot_ublox::{
    commands::*,
    pigeon::{Pigeon, Platform},
    Error,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::VecDeque;

#[derive(Deserialize)]
struct Request {
    mode: String,
    failure: String,
    time_message: Option<Vec<u8>>,
    token: Option<String>,
    assist: Vec<Vec<u8>>,
}
struct Fake {
    request: Request,
    trace: Vec<Value>,
    responses: VecDeque<Vec<u8>>,
    now: f64,
    resets: usize,
}
impl Platform for Fake {
    fn send(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.trace.push(json!(["send", bytes]));
        let response = if self.request.failure == "timeout" {
            Vec::new()
        } else if bytes == RESTORE {
            self.resets += 1;
            let mut reply = UBLOX_BACKUP_RESTORE_MSG.to_vec();
            reply.extend([0, 0, 0, if self.resets == 1 { 2 } else { 3 }]);
            reply
        } else if bytes == SAVE {
            if self.request.failure == "nack" {
                UBLOX_SOS_NACK.to_vec()
            } else {
                UBLOX_SOS_ACK.to_vec()
            }
        } else if bytes.starts_with(&[0xb5, 0x62, 0x13]) {
            UBLOX_ASSIST_ACK.to_vec()
        } else if self.request.failure == "nack" {
            UBLOX_NACK.to_vec()
        } else {
            UBLOX_ACK.to_vec()
        };
        self.responses = VecDeque::from([response]);
        Ok(())
    }
    fn receive(&mut self) -> Result<Vec<u8>, Error> {
        let result = self.responses.pop_front().unwrap_or_default();
        self.trace.push(json!(["receive", result]));
        Ok(result)
    }
    fn baud(&mut self, value: u32) -> Result<(), Error> {
        self.trace.push(json!(["baud", value]));
        Ok(())
    }
    fn power(&mut self, enabled: bool) -> Result<(), Error> {
        self.trace.push(json!(["power", enabled]));
        Ok(())
    }
    fn monotonic(&mut self) -> f64 {
        self.now += 0.2;
        self.now
    }
    fn sleep(&mut self, seconds: f64) -> Result<(), Error> {
        self.trace.push(json!(["sleep", seconds]));
        self.now += seconds;
        Ok(())
    }
    fn log(&mut self, level: &str, text: &str) -> Result<(), Error> {
        self.trace.push(json!(["log", level, text]));
        Ok(())
    }
    fn current_time(&mut self) -> Result<Option<Vec<u8>>, Error> {
        self.trace.push(json!(["time"]));
        Ok(self.request.time_message.clone())
    }
    fn token(&mut self) -> Result<Option<String>, Error> {
        self.trace.push(json!(["token"]));
        Ok(self.request.token.clone())
    }
    fn assist(&mut self, token: &str) -> Result<Vec<Vec<u8>>, Error> {
        self.trace.push(json!(["assist", token]));
        if self.request.failure == "assist" {
            Err(Error::Malformed("assist failed"))
        } else {
            Ok(self.request.assist.clone())
        }
    }
}
fn main() -> Result<(), Error> {
    let request: Request = serde_json::from_reader(std::io::stdin())?;
    let mode = request.mode.clone();
    let mut pigeon = Pigeon {
        platform: Fake {
            request,
            trace: Vec::new(),
            responses: VecDeque::new(),
            now: 0.,
            resets: 0,
        },
    };
    let result = match mode.as_str() {
        "init" => pigeon.init().map(|()| Value::Null),
        "initialize" => pigeon.initialize().map(|v| json!(v)),
        "reset" => pigeon.reset_device().map(|v| json!(v)),
        "save" => pigeon.save_almanac().map(|()| Value::Null),
        _ => return Err(Error::Malformed("trace mode")),
    };
    let result = match result {
        Ok(v) => json!({"value":v}),
        Err(Error::Timeout) => json!({"error":"TimeoutError"}),
        Err(error) => return Err(error),
    };
    serde_json::to_writer(
        std::io::stdout(),
        &json!({"result":result,"trace":pigeon.platform.trace}),
    )?;
    Ok(())
}
