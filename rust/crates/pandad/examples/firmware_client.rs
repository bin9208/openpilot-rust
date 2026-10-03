use openpilot_pandad::{
    firmware::{
        client::{Client, Connection, Environment, Handle, Level},
        Request, Transport,
    },
    health::Health,
    supervisor::{Fault, Panda},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    io::{self, BufRead},
    path::{Path, PathBuf},
    rc::Rc,
};

#[derive(Clone, Deserialize)]
struct DeviceSpec {
    serial: String,
    bootstub: bool,
    bcd: Option<Vec<u8>>,
    spi: bool,
    kind: Vec<u8>,
    versions: Vec<u8>,
    signature: Vec<u8>,
    version: Vec<u8>,
    health: Vec<u8>,
    reads: BTreeMap<String, VecDeque<Vec<u8>>>,
}
#[derive(Deserialize)]
struct Operation {
    name: String,
    #[serde(default)]
    path: Option<PathBuf>,
    #[serde(default)]
    code: Option<Vec<u8>>,
    #[serde(default = "yes")]
    reconnect: bool,
    #[serde(default)]
    bootstub: bool,
    #[serde(default)]
    bootloader: bool,
    #[serde(default = "yes")]
    reset: bool,
    #[serde(default)]
    timeout: Option<f64>,
    #[serde(default)]
    serial: Option<String>,
}
fn yes() -> bool {
    true
}
#[derive(Deserialize)]
struct Input {
    usb: VecDeque<Option<DeviceSpec>>,
    spi: VecDeque<Option<DeviceSpec>>,
    operations: Vec<Operation>,
    file: Vec<u8>,
    file_exists: bool,
    dfu: VecDeque<Vec<Option<String>>>,
    fail_at: Option<usize>,
}
struct Trace {
    calls: Vec<Value>,
    fail_at: Option<usize>,
}
impl Trace {
    fn record(&mut self, value: Value) -> Result<(), Fault> {
        self.calls.push(value);
        if self.fail_at == Some(self.calls.len() - 1) {
            Err(Fault::Other("scripted failure".into()))
        } else {
            Ok(())
        }
    }
}
type Shared = Rc<RefCell<Trace>>;
struct Device {
    spec: DeviceSpec,
    trace: Shared,
}
impl Transport for Device {
    type Error = Fault;
    fn control_read(&mut self, request: Request, length: usize) -> Result<Vec<u8>, Fault> {
        self.trace
            .borrow_mut()
            .record(json!(["read", request, length]))?;
        if let Some(reads) = self.spec.reads.get_mut(&request.request.to_string()) {
            if let Some(value) = reads.pop_front() {
                return Ok(value);
            }
        }
        Ok(match request.request {
            0xc1 => self.spec.kind.clone(),
            0xdd => self.spec.versions.clone(),
            0xd6 => self.spec.version.clone(),
            0xd3 => self.spec.signature.iter().take(64).copied().collect(),
            0xd4 => self.spec.signature.iter().skip(64).copied().collect(),
            0xd2 => self.spec.health.clone(),
            0xb0 => vec![0, 0, 0, 0, 0xde, 0xad, 0xd0, 0x0d, 0, 0, 0, 0],
            _ => vec![0; length],
        })
    }
    fn control_write(&mut self, request: Request, data: &[u8]) -> Result<(), Fault> {
        self.trace
            .borrow_mut()
            .record(json!(["write", request, data]))
    }
    fn bulk_write(&mut self, endpoint: u8, data: &[u8], timeout: u32) -> Result<(), Fault> {
        self.trace
            .borrow_mut()
            .record(json!(["bulk", endpoint, data, timeout]))
    }
}
impl Handle for Device {
    fn close(&mut self) -> Result<(), Fault> {
        self.trace.borrow_mut().record(json!(["close"]))
    }
}
struct Fixture {
    trace: Shared,
    usb: VecDeque<Option<DeviceSpec>>,
    spi: VecDeque<Option<DeviceSpec>>,
    file: Vec<u8>,
    file_exists: bool,
    dfu: VecDeque<Vec<Option<String>>>,
    clock: f64,
}
fn next<T: Clone>(queue: &mut VecDeque<T>) -> T {
    if queue.len() == 1 {
        queue[0].clone()
    } else {
        queue.pop_front().expect("fixture queue is not empty")
    }
}
impl Fixture {
    fn connection(&self, spec: Option<DeviceSpec>) -> Option<Connection<Device>> {
        spec.map(|spec| Connection {
            serial: spec.serial.clone(),
            bootstub: spec.bootstub,
            bcd: spec.bcd.clone(),
            spi: spec.spi,
            handle: Device {
                spec,
                trace: self.trace.clone(),
            },
        })
    }
}
impl Environment for Fixture {
    type Device = Device;
    fn usb_connect(
        &mut self,
        serial: &str,
        claim: bool,
        no_error: bool,
    ) -> Result<Option<Connection<Device>>, Fault> {
        self.trace
            .borrow_mut()
            .record(json!(["usb_connect", serial, claim, no_error]))?;
        let spec = next(&mut self.usb);
        Ok(self.connection(spec))
    }
    fn spi_connect(&mut self, serial: &str) -> Result<Option<Connection<Device>>, Fault> {
        self.trace
            .borrow_mut()
            .record(json!(["spi_connect", serial]))?;
        let spec = next(&mut self.spi);
        Ok(self.connection(spec))
    }
    fn firmware_dir(&self) -> &Path {
        Path::new("/firmware")
    }
    fn file_exists(&mut self, path: &Path) -> Result<bool, Fault> {
        self.trace.borrow_mut().record(json!(["isfile", path]))?;
        Ok(self.file_exists)
    }
    fn file_read(&mut self, path: &Path, tail: Option<usize>) -> Result<Vec<u8>, Fault> {
        self.trace
            .borrow_mut()
            .record(json!(["file", path, tail]))?;
        match tail {
            None => Ok(self.file.clone()),
            Some(size) => self
                .file
                .len()
                .checked_sub(size)
                .map(|offset| self.file[offset..].to_vec())
                .ok_or_else(|| Fault::Other("short file".into())),
        }
    }
    fn log(&mut self, level: Level, text: String) -> Result<(), Fault> {
        self.trace.borrow_mut().record(json!([
            "log",
            match level {
                Level::Debug => "debug",
                Level::Info => "info",
            },
            text
        ]))
    }
    fn sleep(&mut self, seconds: f64) -> Result<(), Fault> {
        self.trace.borrow_mut().record(json!(["sleep", seconds]))?;
        self.clock += seconds;
        Ok(())
    }
    fn monotonic(&mut self) -> Result<f64, Fault> {
        self.trace.borrow_mut().record(json!(["clock"]))?;
        Ok(self.clock)
    }
    fn dfu_list(&mut self) -> Result<Vec<Option<String>>, Fault> {
        self.trace.borrow_mut().record(json!(["dfu_list"]))?;
        Ok(next(&mut self.dfu))
    }
    fn dfu_recover(&mut self, serial: Option<&str>) -> Result<(), Fault> {
        self.trace
            .borrow_mut()
            .record(json!(["dfu_connect", serial]))?;
        self.trace.borrow_mut().record(json!(["dfu_recover"]))
    }
}
fn health_json(health: Health) -> Value {
    serde_json::to_value(health).unwrap()
}
fn run(input: Input) -> Value {
    let trace = Rc::new(RefCell::new(Trace {
        calls: vec![],
        fail_at: input.fail_at,
    }));
    let fixture = Fixture {
        trace: trace.clone(),
        usb: input.usb,
        spi: input.spi,
        file: input.file,
        file_exists: input.file_exists,
        dfu: input.dfu,
        clock: 0.0,
    };
    let mut values = Vec::new();
    let result = (|| {
        let mut client = Client::open(fixture, "010002000300040005000600".into())?;
        for op in input.operations {
            let value = match op.name.as_str() {
                "type" => json!(client.get_type()?),
                "mcu" => json!(client.mcu_type()?),
                "internal" => json!(client.is_internal()?),
                "signature" => json!(client.signature()?),
                "version" => json!(client.version()?),
                "health" => health_json(client.health()?),
                "up_to_date" => json!(client.up_to_date(op.path.as_deref())?),
                "flash" => {
                    client.flash_file(op.path.as_deref(), op.code.as_deref(), op.reconnect)?;
                    Value::Null
                }
                "reset" => {
                    client.reset_to(op.bootstub, op.bootloader, op.reconnect)?;
                    Value::Null
                }
                "reconnect" => {
                    client.reconnect()?;
                    Value::Null
                }
                "recover" => json!(client.recover_with_timeout(op.timeout, op.reset)?),
                "wait" => json!(client.wait_for_dfu(op.serial.as_deref(), op.timeout)?),
                "close" => {
                    client.close()?;
                    Value::Null
                }
                _ => panic!("unknown operation"),
            };
            values.push(
                json!({"value":value,"connected":client.connected(),"bootstub":client.bootstub()}),
            );
        }
        Ok::<_, Fault>(())
    })();
    json!({"ok":result.is_ok(),"values":values,"calls":trace.borrow().calls})
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        println!("{}", run(serde_json::from_str(&line?)?));
    }
    Ok(())
}
