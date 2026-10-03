use openpilot_pandad::{
    health::Health,
    supervisor::{self, Backend, Fault, Log, Panda, Supervisor},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    io::{self, BufRead},
    rc::Rc,
};

#[derive(Clone, Deserialize)]
struct Device {
    serial: String,
    kind: Vec<u8>,
    internal: bool,
    bootstub: bool,
    flash_bootstub: bool,
    recover_bootstub: bool,
    signature: Vec<u8>,
    expected: Vec<u8>,
    final_signature: Vec<u8>,
    version: String,
    health: Health,
}
#[derive(Clone, Deserialize)]
struct Round {
    devices: Vec<Device>,
    dfu: Vec<Option<String>>,
    has_internal: bool,
}
#[derive(Deserialize)]
struct Input {
    operation: String,
    rounds: Vec<Round>,
    fail_at: Option<usize>,
    error_kind: String,
}
struct Trace {
    calls: Vec<Value>,
    fail_at: Option<usize>,
    error_kind: String,
}
impl Trace {
    fn record(&mut self, call: Value) -> Result<(), Fault> {
        self.calls.push(call);
        if self.fail_at == Some(self.calls.len() - 1) {
            let message = "scripted failure".into();
            return Err(match self.error_kind.as_str() {
                "no_device" => Fault::NoDevice(message),
                "pipe" => Fault::Pipe(message),
                "protocol" => Fault::Protocol(message),
                _ => Fault::Other(message),
            });
        }
        Ok(())
    }
}
type Shared = Rc<RefCell<Trace>>;
struct Fixture {
    trace: Shared,
    round: Round,
}
struct Handle {
    trace: Shared,
    device: Device,
}
impl Handle {
    fn record(&self, method: &str) -> Result<(), Fault> {
        self.trace
            .borrow_mut()
            .record(json!([method, self.device.serial]))
    }
}
impl Panda for Handle {
    fn bootstub(&self) -> bool {
        self.device.bootstub
    }
    fn is_internal(&mut self) -> Result<bool, Fault> {
        self.record("is_internal")?;
        Ok(self.device.internal)
    }
    fn get_type(&mut self) -> Result<Vec<u8>, Fault> {
        self.record("get_type")?;
        Ok(self.device.kind.clone())
    }
    fn serial(&mut self) -> Result<String, Fault> {
        self.record("get_usb_serial")?;
        Ok(self.device.serial.clone())
    }
    fn version(&mut self) -> Result<String, Fault> {
        self.record("get_version")?;
        Ok(self.device.version.clone())
    }
    fn signature(&mut self) -> Result<Vec<u8>, Fault> {
        self.record("get_signature")?;
        Ok(self.device.signature.clone())
    }
    fn flash(&mut self) -> Result<(), Fault> {
        self.record("flash")?;
        self.device.bootstub = self.device.flash_bootstub;
        self.device
            .signature
            .clone_from(&self.device.final_signature);
        Ok(())
    }
    fn recover(&mut self, reset: bool) -> Result<(), Fault> {
        self.trace
            .borrow_mut()
            .record(json!(["recover", self.device.serial, reset]))?;
        self.device.bootstub = self.device.recover_bootstub;
        self.device
            .signature
            .clone_from(&self.device.final_signature);
        Ok(())
    }
    fn health(&mut self) -> Result<Health, Fault> {
        self.record("health")?;
        Ok(self.device.health)
    }
    fn reset(&mut self) -> Result<(), Fault> {
        self.trace
            .borrow_mut()
            .record(json!(["reset", self.device.serial, true]))
    }
    fn close(&mut self) -> Result<(), Fault> {
        self.record("close")
    }
}
fn repr(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut output = quote.to_string();
    for ch in value.chars() {
        match ch {
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            ch if ch == quote => {
                output.push('\\');
                output.push(ch);
            }
            ch if ch.is_control() && u32::from(ch) < 256 => {
                output.push_str(&format!("\\x{:02x}", u32::from(ch)))
            }
            _ => output.push(ch),
        }
    }
    output.push(quote);
    output
}
impl Backend for Fixture {
    type Device = Handle;
    fn log(&mut self, entry: Log) -> Result<(), Fault> {
        let (level, message, values) = match entry {
            Log::Info(text) => ("info", text, json!({})),
            Log::Warning(text) => ("warning", text, json!({})),
            Log::Error(text) => ("error", text.into(), json!({})),
            Log::Exception(text, _) => ("exception", text.into(), json!({})),
            Log::Connect { count } => ("event", "pandad.flash_and_connect".into(), json!({"count":count})),
            Log::HeartbeatLost { health, serial } => ("event", "heartbeat lost".into(), json!({"deviceState":health,"serial":serial})),
            Log::SomReset { health, serial } => ("event", "panda.som_reset_triggered".into(), json!({"health":health,"serial":serial})),
            Log::DevelopmentBootloader { version, internal } => ("info", format!(
                "Flashed firmware not booting, flashing development bootloader. bootstub_version={}, internal_panda={}",
                repr(&version), if internal { "True" } else { "False" }), json!({})),
            Log::Found(serials) => ("info", format!("{} panda(s) found, connecting - [{}]", serials.len(),
                serials.iter().map(|s| repr(s)).collect::<Vec<_>>().join(", ")), json!({})),
        };
        self.trace
            .borrow_mut()
            .record(json!(["log", level, message, values]))
    }
    fn remove_signatures(&mut self) -> Result<(), Fault> {
        self.trace
            .borrow_mut()
            .record(json!(["remove", "PandaSignatures"]))
    }
    fn reset_internal(&mut self) -> Result<(), Fault> {
        self.trace.borrow_mut().record(json!(["reset_internal"]))
    }
    fn recover_internal(&mut self) -> Result<(), Fault> {
        self.trace.borrow_mut().record(json!(["recover_internal"]))
    }
    fn sleep(&mut self, seconds: u64) -> Result<(), Fault> {
        self.trace.borrow_mut().record(json!(["sleep", seconds]))
    }
    fn dfu_list(&mut self) -> Result<Vec<Option<String>>, Fault> {
        self.trace.borrow_mut().record(json!(["dfu_list"]))?;
        Ok(self.round.dfu.clone())
    }
    fn dfu_recover(&mut self, serial: Option<&str>) -> Result<(), Fault> {
        self.trace
            .borrow_mut()
            .record(json!(["dfu_connect", serial]))?;
        self.trace
            .borrow_mut()
            .record(json!(["dfu_recover", serial]))
    }
    fn panda_list(&mut self) -> Result<Vec<String>, Fault> {
        self.trace.borrow_mut().record(json!(["list"]))?;
        Ok(self
            .round
            .devices
            .iter()
            .map(|d| d.serial.clone())
            .collect())
    }
    fn connect(&mut self, serial: &str) -> Result<Handle, Fault> {
        self.trace.borrow_mut().record(json!(["connect", serial]))?;
        Ok(Handle {
            trace: self.trace.clone(),
            device: self
                .round
                .devices
                .iter()
                .find(|d| d.serial == serial)
                .unwrap()
                .clone(),
        })
    }
    fn expected_signature(&mut self, panda: &mut Handle) -> Result<Vec<u8>, Fault> {
        panda.record("get_mcu_type")?;
        self.trace
            .borrow_mut()
            .record(json!(["firmware_signature", "/firmware/app.bin"]))?;
        Ok(panda.device.expected.clone())
    }
    fn has_internal(&mut self) -> Result<bool, Fault> {
        self.trace.borrow_mut().record(json!(["has_internal"]))?;
        Ok(self.round.has_internal)
    }
    fn put_signatures(&mut self, data: &[u8]) -> Result<(), Fault> {
        self.trace
            .borrow_mut()
            .record(json!(["put", "PandaSignatures", data]))
    }
    fn put_bool(&mut self, key: &str) -> Result<(), Fault> {
        self.trace
            .borrow_mut()
            .record(json!(["put_bool", key, true]))
    }
    fn run_child(&mut self, serials: &[String]) -> Result<(), Fault> {
        self.trace
            .borrow_mut()
            .record(json!(["env", "MANAGER_DAEMON", "pandad"]))?;
        self.trace.borrow_mut().record(json!(["spawn", serials]))?;
        self.trace.borrow_mut().record(json!(["wait"]))
    }
}
fn run(input: Input) -> Value {
    let trace = Rc::new(RefCell::new(Trace {
        calls: vec![],
        fail_at: input.fail_at,
        error_kind: input.error_kind,
    }));
    let mut supervisor = Supervisor::default();
    let mut result = Ok(());
    for round in input.rounds {
        let mut fixture = Fixture {
            trace: trace.clone(),
            round,
        };
        result = match input.operation.as_str() {
            "main" => supervisor.step(&mut fixture),
            "flash" => {
                let serial = fixture.round.devices[0].serial.clone();
                supervisor::flash_panda(&mut fixture, &serial).map(|_| ())
            }
            "all" => {
                let serials = fixture
                    .round
                    .devices
                    .iter()
                    .map(|d| d.serial.clone())
                    .collect::<Vec<_>>();
                supervisor::flash_all(&mut fixture, &serials).map(|_| ())
            }
            _ => panic!("invalid fixture operation"),
        };
        if result.is_err() {
            break;
        }
    }
    json!({"ok":result.is_ok(),"calls":trace.borrow().calls})
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        println!("{}", run(serde_json::from_str(&line?)?));
    }
    Ok(())
}
