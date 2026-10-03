use openpilot_pandad::firmware::{
    dfu_spi::DfuSpi,
    spi::{Error, Io, Mode, PandaSpi},
    Mcu, Request,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    io::{self, BufRead},
    rc::Rc,
};

#[derive(Deserialize)]
struct Wire {
    hack: Vec<u8>,
    dack: Vec<u8>,
    tail: Vec<u8>,
    version_echo: Vec<u8>,
    version_reply: Vec<u8>,
    dfu_ack: Vec<u8>,
    dfu_reply: Vec<u8>,
    kernel_reply: Vec<u8>,
}
#[derive(Deserialize)]
struct Input {
    operation: String,
    wire: Wire,
    fail_at: Option<usize>,
    fail_kind: String,
    tick: f64,
    kernel: bool,
    data: Vec<u8>,
    endpoint: u8,
    timeout: u32,
    maximum: usize,
    disconnect: bool,
    request: Request,
    serial: Option<String>,
    ignore_version: bool,
    mcu: Mcu,
    address: u32,
    length: usize,
    sector: u16,
    command: u8,
    parts: Option<Vec<Vec<u8>>>,
    predata: Option<Vec<u8>>,
    ack: u8,
    tx: u8,
    ack_timeout: f64,
}
struct Trace {
    calls: Vec<Value>,
    fail_at: Option<usize>,
    fail_kind: String,
}
impl Trace {
    fn record(&mut self, value: Value) -> Result<(), Error> {
        self.calls.push(value);
        if self.fail_at == Some(self.calls.len() - 1) {
            return Err(match self.fail_kind.as_str() {
                "nack" => Error::Nack,
                "missing_ack" => Error::MissingAck,
                "checksum" => Error::BadChecksum,
                "protocol" => Error::Protocol("scripted failure".into()),
                _ => Error::Io("scripted failure".into()),
            });
        }
        Ok(())
    }
}
struct Fixture {
    trace: Rc<RefCell<Trace>>,
    wire: Wire,
    clock: f64,
    tick: f64,
    version_stage: Option<bool>,
}
impl Io for Fixture {
    fn lock(&mut self) -> Result<(), Error> {
        self.trace.borrow_mut().record(json!(["lock"]))
    }
    fn unlock(&mut self) -> Result<(), Error> {
        self.trace.borrow_mut().record(json!(["unlock"]))
    }
    fn transfer(&mut self, mode: Mode, data: &[u8]) -> Result<Vec<u8>, Error> {
        self.trace.borrow_mut().record(json!([
            match mode {
                Mode::Xfer => "xfer",
                Mode::Xfer2 => "xfer2",
            },
            data
        ]))?;
        Ok(match mode {
            Mode::Xfer2 if data == [0x11] => self.wire.hack.clone(),
            Mode::Xfer2 if data.len() == 68 && data.iter().all(|v| *v == 0x13) => {
                self.wire.dack.clone()
            }
            Mode::Xfer if data == [0] => self.wire.dfu_ack.clone(),
            Mode::Xfer if data.len() > 1 && data.iter().all(|v| *v == 0) => self
                .wire
                .dfu_reply
                .iter()
                .copied()
                .chain(std::iter::repeat(0))
                .take(data.len())
                .collect(),
            _ => vec![0; data.len()],
        })
    }
    fn read(&mut self, length: usize) -> Result<Vec<u8>, Error> {
        self.trace.borrow_mut().record(json!(["read", length]))?;
        Ok(match self.version_stage {
            Some(false) => {
                if self.wire.version_echo.starts_with(b"VERSION") {
                    self.version_stage = Some(true);
                }
                self.wire.version_echo.clone()
            }
            Some(true) => {
                self.version_stage = None;
                self.wire.version_reply.clone()
            }
            None => self
                .wire
                .tail
                .iter()
                .copied()
                .chain(std::iter::repeat(0))
                .take(length)
                .collect(),
        })
    }
    fn write(&mut self, data: &[u8]) -> Result<(), Error> {
        self.trace.borrow_mut().record(json!(["write", data]))?;
        if data == b"VERSION" {
            self.version_stage = Some(false);
        }
        Ok(())
    }
    fn now(&mut self) -> Result<f64, Error> {
        self.trace.borrow_mut().record(json!(["clock"]))?;
        self.clock += self.tick;
        Ok(self.clock)
    }
    fn sleep(&mut self, seconds: f64) -> Result<(), Error> {
        self.trace.borrow_mut().record(json!(["sleep", seconds]))?;
        self.clock += seconds;
        Ok(())
    }
    fn log(&mut self, text: String, exception: Option<&Error>) -> Result<(), Error> {
        self.trace
            .borrow_mut()
            .record(json!(["log", text, exception.is_some()]))
    }
    fn kernel(
        &mut self,
        endpoint: u8,
        data: &[u8],
        maximum: usize,
        disconnect: bool,
    ) -> Result<Vec<u8>, Error> {
        self.trace
            .borrow_mut()
            .record(json!(["kernel", endpoint, data, maximum, disconnect, 0]))
            .map_err(|error| match error {
                Error::Io(_) => Error::Protocol("kernel ioctl error".into()),
                error => error,
            })?;
        Ok(self.wire.kernel_reply.clone())
    }
}

fn run(input: Input) -> Value {
    let trace = Rc::new(RefCell::new(Trace {
        calls: vec![],
        fail_at: input.fail_at,
        fail_kind: input.fail_kind,
    }));
    let io = Fixture {
        trace: trace.clone(),
        wire: input.wire,
        clock: 0.0,
        tick: input.tick,
        version_stage: None,
    };
    let result: Result<Value, Error> = (|| {
        if input.operation == "dfu_probe" {
            return DfuSpi::probe(io).map(|dfu| json!(dfu.mcu));
        }
        if input.operation.starts_with("dfu_") {
            let mut dfu = DfuSpi { io, mcu: input.mcu };
            return match input.operation.as_str() {
                "dfu_ack" => dfu.ack(input.ack_timeout).map(|()| Value::Null),
                "dfu_command" => dfu
                    .command(
                        input.command,
                        input.parts.as_deref(),
                        input.length,
                        input.predata.as_deref(),
                    )
                    .map(|v| json!(v)),
                "dfu_once" => dfu
                    .command_once(
                        input.command,
                        input.parts.as_deref(),
                        input.length,
                        input.predata.as_deref(),
                    )
                    .map(|v| json!(v)),
                "dfu_read" => dfu.read(input.address, input.length).map(|v| json!(v)),
                "dfu_chip" => dfu.chip_id().map(|v| json!(v)),
                "dfu_uid" => dfu.uid().map(|v| json!(v)),
                "dfu_erase" => dfu.erase_sector(input.sector).map(|()| Value::Null),
                "dfu_program" => dfu
                    .program(input.address, &input.data)
                    .map(|()| Value::Null),
                "dfu_jump" => dfu.jump().map(|()| Value::Null),
                "dfu_recover" => dfu.recover(&input.data).map(|()| Value::Null),
                _ => panic!("unknown DFU fixture operation"),
            };
        }
        let mut spi = PandaSpi::new(io, input.kernel);
        match input.operation.as_str() {
            "ack" => spi
                .wait_ack(input.ack, input.timeout, input.tx, input.length)
                .map(|v| json!(v)),
            "transfer" => spi
                .transfer(
                    input.endpoint,
                    &input.data,
                    input.timeout,
                    input.maximum,
                    input.disconnect,
                )
                .map(|v| json!(v)),
            "once" => spi
                .transfer_spidev(
                    input.endpoint,
                    &input.data,
                    input.timeout,
                    input.maximum,
                    input.disconnect,
                )
                .map(|v| json!(v)),
            "protocol" => spi.protocol_version().map(|v| json!(v)),
            "identify" => spi
                .identify(input.serial.as_deref(), input.ignore_version)
                .map(|v| {
                    v.map_or(
                        Value::Null,
                        |id| json!({"serial":id.serial,"bootstub":id.bootstub}),
                    )
                }),
            "read" => spi
                .read_control(input.request, input.length)
                .map(|v| json!(v)),
            "write" => spi.write_control(input.request).map(|v| json!(v)),
            "bulk_read" => spi
                .read_bulk(input.endpoint, input.length, input.timeout)
                .map(|v| json!(v)),
            "bulk_write" => spi
                .write_bulk(input.endpoint, &input.data, input.timeout)
                .map(|v| json!(v)),
            _ => panic!("unknown SPI fixture operation"),
        }
    })();
    match result {
        Ok(value) => json!({"ok":true,"value":value,"calls":trace.borrow().calls}),
        Err(error) => {
            json!({"ok":false,"error_kind":match error { Error::Nack=>"nack",Error::MissingAck=>"missing_ack",Error::BadChecksum=>"checksum",
            Error::Protocol(_)=>"protocol",Error::Io(_)=>"io",Error::Version(_)=>"version",Error::Invalid(_)=>"invalid" },"calls":trace.borrow().calls})
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        println!("{}", run(serde_json::from_str(&line?)?));
    }
    Ok(())
}
