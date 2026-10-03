use openpilot_pandad::firmware::{self, dfu_usb::DfuUsb, Mcu, Request, Transport};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    cell::Cell,
    collections::VecDeque,
    io::{self, BufRead},
    rc::Rc,
};

#[derive(Deserialize)]
struct Input {
    operation: String,
    mcu: Mcu,
    #[serde(default)]
    size: usize,
    #[serde(default)]
    seed: u8,
    #[serde(default)]
    serial: String,
    #[serde(default)]
    sector: usize,
    #[serde(default)]
    fail_at: Option<usize>,
    #[serde(default)]
    progress_fail_at: Option<usize>,
    #[serde(default)]
    reads: VecDeque<Vec<u8>>,
}

struct Fixture {
    calls: Vec<Value>,
    call_count: Rc<Cell<usize>>,
    fail_at: Option<usize>,
    reads: VecDeque<Vec<u8>>,
}

impl Fixture {
    fn record(&mut self, value: Value) -> Result<(), &'static str> {
        self.calls.push(value);
        self.call_count.set(self.calls.len());
        if self.fail_at == Some(self.calls.len() - 1) {
            Err("scripted transport failure")
        } else {
            Ok(())
        }
    }
}

impl Transport for Fixture {
    type Error = &'static str;
    fn control_read(&mut self, request: Request, length: usize) -> Result<Vec<u8>, Self::Error> {
        self.record(json!({"op":"read","request":request,"length":length}))?;
        Ok(self.reads.pop_front().unwrap_or_else(|| {
            if request.kind == 0xc0 && request.request == 0xb0 {
                vec![0, 0, 0, 0, 0xde, 0xad, 0xd0, 0x0d, 0, 0, 0, 0]
            } else {
                vec![0; length]
            }
        }))
    }
    fn control_write(&mut self, request: Request, data: &[u8]) -> Result<(), Self::Error> {
        self.record(json!({"op":"write","request":request,"data":data}))
    }
    fn bulk_write(
        &mut self,
        endpoint: u8,
        data: &[u8],
        timeout_ms: u32,
    ) -> Result<(), Self::Error> {
        self.record(json!({"op":"bulk","endpoint":endpoint,"data":data,"timeout_ms":timeout_ms}))
    }
}

fn run(input: Input) -> Value {
    let call_count = Rc::new(Cell::new(0));
    let mut progress = Vec::new();
    let mut fixture = Fixture {
        calls: Vec::new(),
        call_count: call_count.clone(),
        fail_at: input.fail_at,
        reads: input.reads,
    };
    let data: Vec<_> = (0..input.size)
        .map(|index| (index as u8).wrapping_mul(37).wrapping_add(input.seed))
        .collect();
    let result: Result<Value, String> = if input.operation == "serial" {
        firmware::dfu_serial(&input.serial, input.mcu)
            .map(|value| json!(value))
            .map_err(|error| error.to_string())
    } else if input.operation == "flash" {
        firmware::flash_static(&mut fixture, &data, input.mcu)
            .map(|()| Value::Null)
            .map_err(|error| error.to_string())
    } else {
        let mut dfu = DfuUsb {
            transport: &mut fixture,
            mcu: input.mcu,
        };
        let result = match input.operation.as_str() {
            "clear" => dfu.clear_status(),
            "erase" => dfu.erase_sector(input.sector),
            "program" => dfu.program_with_progress(0x0800_0000, &data, |text| {
                progress.push(json!({"before_call":call_count.get(), "text":text}));
                if input.progress_fail_at == Some(progress.len() - 1) {
                    Err("scripted stdout failure")
                } else {
                    Ok(())
                }
            }),
            "jump" => dfu.jump(0x0800_0000),
            "recover" => dfu.recover_with_progress(&data, |text| {
                progress.push(json!({"before_call":call_count.get(), "text":text}));
                if input.progress_fail_at == Some(progress.len() - 1) {
                    Err("scripted stdout failure")
                } else {
                    Ok(())
                }
            }),
            _ => panic!("unknown fixture operation"),
        };
        result
            .map(|()| Value::Null)
            .map_err(|error| error.to_string())
    };
    match result {
        Ok(value) => json!({"ok":true,"value":value,"calls":fixture.calls,"progress":progress}),
        Err(error) => json!({"ok":false,"error":error,"calls":fixture.calls,"progress":progress}),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        println!("{}", run(serde_json::from_str(&line?)?));
    }
    Ok(())
}
