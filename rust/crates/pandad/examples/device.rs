use openpilot_pandad::device::{Control, Device, Transport};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    io::{self, BufRead},
    sync::Mutex,
};

#[derive(Deserialize)]
struct Reply {
    request: u8,
    count: i32,
    bytes: Vec<u8>,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Operation {
    Health,
    CanHealth { bus: u16 },
    FanSpeed,
    Signature,
    Serial,
    SerialRead { port: u16 },
}
#[derive(Deserialize)]
struct Request {
    replies: VecDeque<Reply>,
    operations: Vec<Operation>,
}
struct Recorder {
    replies: Mutex<VecDeque<Reply>>,
    calls: Mutex<Vec<Value>>,
}

impl Transport for Recorder {
    type Error = io::Error;
    fn control_read(&self, command: Control, output: &mut [u8]) -> Result<i32, Self::Error> {
        self.calls.lock().map_err(|_| io::Error::other("fixture mutex poisoned"))?
            .push(json!({"read":true,"request":command.request,"value":command.value,
                         "index":command.index,"timeout_ms":command.timeout_ms,"length":output.len()}));
        let reply = self
            .replies
            .lock()
            .map_err(|_| io::Error::other("fixture mutex poisoned"))?
            .pop_front()
            .ok_or_else(|| io::Error::other("unexpected control read"))?;
        if reply.request != command.request || reply.bytes.len() > output.len() {
            return Err(io::Error::other("control read contract mismatch"));
        }
        output[..reply.bytes.len()].copy_from_slice(&reply.bytes);
        Ok(reply.count)
    }
    fn control_write(&self, command: Control) -> Result<i32, Self::Error> {
        self.calls
            .lock()
            .map_err(|_| io::Error::other("fixture mutex poisoned"))?
            .push(
                json!({"read":false,"request":command.request,"value":command.value,
                         "index":command.index,"timeout_ms":command.timeout_ms,"length":0}),
            );
        Ok(0)
    }
}

fn run(request: Request) -> Result<Value, Box<dyn std::error::Error>> {
    let recorder = Recorder {
        replies: Mutex::new(request.replies),
        calls: Mutex::new(Vec::new()),
    };
    let device = Device::connect(recorder, 4)?;
    let mut results = Vec::new();
    for operation in request.operations {
        let result = match operation {
            Operation::Health => device.health()?.map_or(
                Value::Null,
                |health| json!({"health":health,"interrupt_bits":health.interrupt_load.to_bits()}),
            ),
            Operation::CanHealth { bus } => serde_json::to_value(device.can_health(bus)?)?,
            Operation::FanSpeed => json!(device.fan_speed()?),
            Operation::Signature => device
                .firmware_signature()?
                .map_or(Value::Null, |bytes| json!(bytes.as_slice())),
            Operation::Serial => serde_json::to_value(device.serial()?)?,
            Operation::SerialRead { port } => json!(device.serial_read(port)?),
        };
        results.push(result);
    }
    let remaining = device
        .transport()
        .replies
        .lock()
        .map_err(|_| io::Error::other("fixture mutex poisoned"))?
        .len();
    let calls = device
        .transport()
        .calls
        .lock()
        .map_err(|_| io::Error::other("fixture mutex poisoned"))?;
    Ok(
        json!({"hardware_type":device.hardware_type(),"remaining":remaining,"results":results,"calls":*calls}),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        println!("{}", run(serde_json::from_str(&line?)?)?);
    }
    Ok(())
}
