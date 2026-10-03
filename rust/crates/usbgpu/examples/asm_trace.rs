use openpilot_usbgpu::{
    bus_lock::BusLock,
    clock::Clock,
    custom_asm::CustomAsm,
    transport::{BulkResult, Control, Description, Setup, Transfer, Transport},
    usb3::Usb3,
    Error,
};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    io::{self, BufRead},
    time::Duration,
};
struct Fixture {
    replies: VecDeque<Value>,
    trace: Vec<Value>,
}
impl Transport for Fixture {
    fn describe(&self) -> Result<Description, Error> {
        Ok(Description {
            bus: 1,
            address: 1,
            product: b"custom fixture".to_vec(),
        })
    }
    fn setup(&mut self, _: Setup, _: i32, _: i32) -> Result<i32, Error> {
        Ok(0)
    }
    fn streams(&mut self, _: &[u8], _: u32) -> Result<i32, Error> {
        unreachable!()
    }
    fn control(&mut self, c: Control, data: &mut [u8]) -> Result<i32, Error> {
        self.trace.push(json!({"kind":c.kind,"request":c.request,"value":c.value,"index":c.index,"timeout":c.timeout_ms,"length":data.len(),"data":if c.kind&0x80==0 {Some(data.to_vec())} else {None}}));
        let reply = self.replies.pop_front().expect("missing control reply");
        if let Some(values) = reply.get("data").and_then(Value::as_array) {
            for (target, value) in data.iter_mut().zip(values) {
                *target = value.as_u64().unwrap() as u8;
            }
        }
        Ok(reply
            .get("code")
            .and_then(Value::as_i64)
            .map_or(data.len() as i32, |n| n as i32))
    }
    fn bulk(&mut self, endpoint: u8, data: &mut [u8], timeout: u32) -> Result<BulkResult, Error> {
        self.trace.push(json!({"bulk":endpoint,"length":data.len(),"timeout":timeout,"data":if endpoint&0x80==0 {Some(data.to_vec())} else {None}}));
        let reply = self.replies.pop_front().unwrap();
        if let Some(values) = reply.get("data").and_then(Value::as_array) {
            for (target, value) in data.iter_mut().zip(values) {
                *target = value.as_u64().unwrap() as u8;
            }
        }
        Ok(BulkResult {
            code: reply.get("code").and_then(Value::as_i64).unwrap_or(0) as i32,
            actual: reply
                .get("actual")
                .and_then(Value::as_u64)
                .unwrap_or(data.len() as u64) as u32,
        })
    }
    fn batch(&mut self, _: &mut [Transfer]) -> Result<(), Error> {
        unreachable!()
    }
    fn error_text(&self, _: i32) -> String {
        "fixture".into()
    }
}
#[derive(Default)]
struct Time {
    waits: Vec<u64>,
}
impl Clock for Time {
    fn now(&self) -> Duration {
        Duration::from_millis(self.waits.iter().sum())
    }
    fn sleep(&mut self, d: Duration) {
        self.waits.push(d.as_millis() as u64);
    }
}
fn main() {
    for line in io::stdin().lock().lines() {
        let input: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let usb = Usb3::new(
            Fixture {
                replies: input["replies"].as_array().unwrap().clone().into(),
                trace: Vec::new(),
            },
            Time::default(),
            BusLock::open(&temp.path().join("lock")).unwrap(),
            true,
        )
        .unwrap();
        let mut controller = CustomAsm::new(usb).unwrap();
        let mut values = Vec::new();
        let mut error = None;
        for operation in input["operations"].as_array().unwrap() {
            let address = operation
                .get("address")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let length = operation.get("length").and_then(Value::as_u64).unwrap_or(0) as usize;
            let result = match operation["kind"].as_str().unwrap() {
                "read" => controller.read(address as u16, length).map(|v| json!(v)),
                "write" => controller
                    .write(address as u16, &bytes(&operation["data"]))
                    .map(|()| Value::Null),
                "power" => controller
                    .power(operation["on"].as_bool().unwrap())
                    .map(|()| Value::Null),
                "request" => controller
                    .request(
                        operation["format"].as_u64().unwrap() as u8,
                        address,
                        operation
                            .get("value")
                            .and_then(Value::as_u64)
                            .map(|v| v as u32),
                        operation["size"].as_u64().unwrap() as u8,
                    )
                    .map(|v| json!(v)),
                "cache" => {
                    controller.cache_range(address, length as u64);
                    Ok(Value::Null)
                }
                "memory_read" => controller.memory_read(address, length).map(|v| json!(v)),
                "memory_write" => controller
                    .memory_write(
                        address,
                        &operation["data"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| v.as_u64().unwrap() as u32)
                            .collect::<Vec<_>>(),
                    )
                    .map(|()| Value::Null),
                "scsi_write" => controller
                    .scsi_write(&bytes(&operation["data"]))
                    .map(|()| Value::Null),
                "scsi_read_arm" => controller.scsi_read_arm(length).map(|()| Value::Null),
                "scsi_read" => controller.scsi_read(length).map(|v| json!(v)),
                _ => panic!("unknown fixture operation"),
            };
            match result {
                Ok(v) => values.push(v),
                Err(e) => {
                    error = Some(e.to_string());
                    break;
                }
            }
        }
        println!(
            "{}",
            json!({"values":values,"error":error,"trace":controller.usb.transport.trace,"waits":controller.usb.clock.waits,"remaining":controller.usb.transport.replies.len()})
        );
    }
}
fn bytes(value: &Value) -> Vec<u8> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u8)
        .collect()
}
