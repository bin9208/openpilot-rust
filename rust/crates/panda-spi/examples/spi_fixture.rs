use openpilot_panda_spi::protocol::Clock;
use openpilot_panda_spi::{
    device::{Device, Shared},
    linux_io::{CallResult, Kernel, LinuxIo, OptionKind},
};
use serde_json::{json, Value};
use std::{
    io::{self, BufRead},
    sync::{Arc, Mutex},
};

struct Fixture {
    scenario: Value,
    calls: Vec<Value>,
    logs: Vec<Value>,
    index: usize,
    defined: usize,
    unspecified: usize,
    timestamp: u64,
    tick: u64,
    opened: bool,
}
#[derive(Clone)]
struct Mock(Arc<Mutex<Fixture>>);
fn number(value: &Value, key: &str) -> i32 {
    value[key].as_i64().unwrap_or(0) as i32
}
impl Fixture {
    fn step(&mut self) -> Value {
        let step = self.scenario["steps"][self.index].clone();
        assert!(!step.is_null(), "SPI fixture script exhausted");
        self.index += 1;
        step
    }
}
impl Clock for Mock {
    fn now_ns(&self) -> u64 {
        let mut f = self.0.lock().unwrap();
        f.timestamp += f.tick;
        f.timestamp
    }
}
impl Kernel for Mock {
    type Clock = Mock;
    fn clock(&self) -> Self::Clock {
        self.clone()
    }
    fn exists(&mut self) -> bool {
        let mut f = self.0.lock().unwrap();
        f.calls.push(json!(["stat", "/dev/spidev0.0"]));
        number(&f.scenario["stat"], "result") != -1
    }
    fn open(&mut self) -> i32 {
        let mut f = self.0.lock().unwrap();
        f.calls.push(json!(["open", "/dev/spidev0.0", 2]));
        let fd = f.scenario["open"]["result"].as_i64().unwrap_or(517) as i32;
        f.opened = fd >= 0;
        fd
    }
    fn configure(&mut self, option: OptionKind, value: u32) -> CallResult {
        let kind = match option {
            OptionKind::Mode => "mode",
            OptionKind::Speed => "speed",
            OptionKind::Bits => "bits",
        };
        let mut f = self.0.lock().unwrap();
        let step = f.step();
        assert_eq!(step["kind"], kind);
        f.calls.push(json!([kind, value]));
        CallResult {
            result: number(&step, "result"),
            errno: number(&step, "errno"),
            fd: 517,
            request: match option {
                OptionKind::Mode => 0x40016b01,
                OptionKind::Speed => 0x40046b04,
                OptionKind::Bits => 0x40016b03,
            },
            argument_address: 1,
        }
    }
    fn errno_description(&self, errno: i32) -> String {
        match errno {
            1 => "Operation not permitted",
            5 => "Input/output error",
            13 => "Permission denied",
            _ => panic!("fixture errno text missing"),
        }
        .into()
    }
    fn error_probability(&mut self) -> Result<f64, String> {
        Ok(-1.0)
    }
    fn random(&mut self) -> u32 {
        panic!("fault injection not requested")
    }
    fn diagnostic_print(&mut self, _: &str) {
        panic!("fault injection not requested")
    }

    fn now_ns(&mut self) -> u64 {
        let mut fixture = self.0.lock().unwrap();
        fixture.timestamp += fixture.tick;
        fixture.timestamp
    }
    fn transfer(&mut self, tx: &[u8], rx: &mut [u8]) -> CallResult {
        let mut f = self.0.lock().unwrap();
        let step = f.step();
        assert_eq!(step["kind"], "transfer");
        assert_eq!(tx.len(), rx.len());
        if let Some(expected) = step["tx"].as_array() {
            assert_eq!(json!(tx), step["tx"]);
            f.defined = f.defined.max(expected.len());
        }
        if let Some(length) = step["length"].as_u64() {
            assert_eq!(tx.len() as u64, length);
        }
        let masked: Vec<Value> = tx
            .iter()
            .enumerate()
            .map(|(i, byte)| {
                if i < f.defined {
                    json!(byte)
                } else {
                    f.unspecified += 1;
                    Value::Null
                }
            })
            .collect();
        f.calls.push(json!(["transfer", tx.len(), masked]));
        rx.fill(0);
        for (i, value) in step["rx"].as_array().unwrap().iter().enumerate() {
            rx[i] = value.as_u64().unwrap() as u8;
        }
        f.timestamp += step["elapsed_ns"].as_u64().unwrap_or(0);
        CallResult {
            result: step["result"]
                .as_i64()
                .map_or(tx.len() as i32, |value| value as i32),
            errno: number(&step, "errno"),
            ..CallResult::default()
        }
    }
    fn log(&mut self, level: u8, message: String) {
        self.0
            .lock()
            .unwrap()
            .logs
            .push(json!({"level":level,"message":message}));
    }
    fn flock(&mut self, exclusive: bool) {
        self.0
            .lock()
            .unwrap()
            .calls
            .push(json!(["flock", if exclusive { 2 } else { 8 }]));
    }
    fn yield_now(&mut self) {
        self.0.lock().unwrap().calls.push(json!("yield"));
    }
    fn sleep_us(&mut self, micros: u32) {
        let mut f = self.0.lock().unwrap();
        f.calls.push(json!(["sleep", micros]));
        f.timestamp += u64::from(micros) * 1000;
    }
    fn close(&mut self) {
        let mut f = self.0.lock().unwrap();
        if f.opened {
            f.calls.push(json!("close"));
            f.opened = false;
        }
    }
}
fn event(shared: &Shared) -> Value {
    let mut value = serde_json::to_value(shared.error_event()).unwrap();
    value["published_sequence"] = json!(shared.error_sequence());
    value
}
fn run(
    mock: Mock,
    shared: Arc<Shared>,
    scenario: &Value,
    output: &mut Value,
    outcomes: &mut Vec<Value>,
) -> Result<(), String> {
    let io = LinuxIo::open(mock).map_err(|e| e.to_string())?;
    let device = Device::connect(
        io,
        Arc::clone(&shared),
        scenario["serial"].as_str().unwrap_or(""),
    )
    .map_err(|e| e.to_string())?;
    if scenario["list"].as_bool().unwrap_or(false) {
        output["serials"] = json!([device.serial()]);
        return Ok(());
    }
    output["serial"] = json!(device.serial());
    for op in scenario["operations"].as_array().unwrap() {
        let mut data = vec![0xa5; op["length"].as_u64().unwrap_or(0) as usize];
        if let Some(input) = op["data"].as_array() {
            for (i, byte) in input.iter().enumerate() {
                data[i] = byte.as_u64().unwrap() as u8;
            }
        }
        let request = number(op, "request") as u8;
        let param1 = number(op, "param1") as u16;
        let param2 = number(op, "param2") as u16;
        let timeout = op["timeout"].as_u64().unwrap_or(0) as u32;
        let endpoint = number(op, "endpoint") as u8;
        let result = match op["kind"].as_str().unwrap() {
            "control_read" => device.control_read(request, param1, param2, &mut data, timeout),
            "control_write" => device.control_write(request, param1, param2, timeout),
            "bulk_read" => device.bulk_read(endpoint, &mut data, timeout),
            "bulk_write" => device.bulk_write(endpoint, &data, timeout),
            kind => panic!("unknown operation {kind}"),
        }
        .map_err(|e| e.to_string())?;
        outcomes.push(json!({"return":result,"data":data,"connected":device.connected(),"healthy":device.healthy(),"event":event(&shared)}));
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let shared = Arc::new(Shared::default());
    let mut timestamp = 1_000_000_000;
    for line in io::stdin().lock().lines() {
        let scenario: Value = serde_json::from_str(&line?)?;
        let mock = Mock(Arc::new(Mutex::new(Fixture {
            tick: scenario["tick_ns"].as_u64().unwrap_or(100_000),
            scenario: scenario.clone(),
            calls: vec![],
            logs: vec![],
            index: 0,
            defined: 0,
            unspecified: 0,
            timestamp,
            opened: false,
        })));
        let mut output = json!({});
        let mut outcomes = vec![];
        if let Err(error) = run(
            mock.clone(),
            Arc::clone(&shared),
            &scenario,
            &mut output,
            &mut outcomes,
        ) {
            if scenario["list"].as_bool().unwrap_or(false) {
                output["serials"] = json!([]);
            } else {
                output["error"] = json!(error);
            }
        }
        let fixture = mock.0.lock().unwrap();
        assert_eq!(
            fixture.index,
            scenario["steps"].as_array().unwrap().len(),
            "unconsumed SPI steps"
        );
        output["outcomes"] = json!(outcomes);
        output["calls"] = json!(fixture.calls);
        output["logs"] = json!(fixture.logs);
        output["clock_ns"] = json!(fixture.timestamp);
        output["unspecified_tx_bytes"] = json!(fixture.unspecified);
        output["event"] = event(&shared);
        timestamp = fixture.timestamp;
        println!("{output}");
    }
    Ok(())
}
