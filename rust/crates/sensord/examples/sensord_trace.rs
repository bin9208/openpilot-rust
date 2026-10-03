mod support;
use openpilot_sensord::{
    clock::Ratekeeper,
    loops::{self, Interrupt, Poll, Sink},
    sensor::{Kind, Sensor},
    wire, Bus, Clock, Error,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{cell::RefCell, rc::Rc};
#[derive(Deserialize)]
#[serde(default)]
struct Request {
    chip: u8,
    self_test: Option<String>,
    fail_self_test: bool,
    mode: String,
    frames: Vec<Frame>,
}
impl Default for Request {
    fn default() -> Self {
        Self {
            chip: 0x6a,
            self_test: None,
            fail_self_test: false,
            mode: "drivers".into(),
            frames: Vec::new(),
        }
    }
}
#[derive(Deserialize)]
#[serde(default)]
struct Frame {
    mono: f64,
    offset: i128,
    poll: String,
    timestamp: u64,
    ready: u8,
    fault: Option<u8>,
}
impl Default for Frame {
    fn default() -> Self {
        Self {
            mono: 1.,
            offset: 1_000_000_000,
            poll: "data".into(),
            timestamp: 3_000_000_000,
            ready: 3,
            fault: None,
        }
    }
}
struct State {
    registers: [u8; 256],
    trace: Vec<Value>,
    mono: f64,
    offset: i128,
    ready: u8,
    fault: Option<u8>,
    fail_test: bool,
}
struct FakeBus {
    index: u8,
    state: Rc<RefCell<State>>,
}
impl Bus for FakeBus {
    fn read(&mut self, reg: u8, len: usize) -> Result<Vec<u8>, Error> {
        let mut s = self.state.borrow_mut();
        s.trace.push(json!([self.index, "read", reg, len]));
        if s.fault == Some(reg) {
            s.fault = None;
            return Err(std::io::Error::from_raw_os_error(5).into());
        }
        if reg == 0x1e {
            return Ok(vec![s.ready]);
        }
        let values = match reg {
            0x28 => Some(if s.registers[0x14] & 3 != 0 && !s.fail_test {
                [2100i16, 1800, 18384]
            } else {
                [100i16, -200, 16384]
            }),
            0x22 => Some(if s.registers[0x14] & 12 != 0 && !s.fail_test {
                [4900i16, 5100, 5200]
            } else {
                [-100i16, 100, 200]
            }),
            _ => None,
        };
        if let Some(values) = values {
            return Ok(values
                .into_iter()
                .flat_map(i16::to_le_bytes)
                .take(len)
                .collect());
        }
        if reg == 0x20 {
            return Ok((-256i16).to_le_bytes().to_vec());
        }
        Ok((0..len)
            .map(|i| s.registers[(usize::from(reg) + i) % 256])
            .collect())
    }
    fn write(&mut self, reg: u8, value: u8) -> Result<(), Error> {
        let mut s = self.state.borrow_mut();
        s.trace.push(json!([self.index, "write", reg, value]));
        s.registers[usize::from(reg)] = value;
        Ok(())
    }
}
struct FakeClock(Rc<RefCell<State>>);
impl Clock for FakeClock {
    fn monotonic(&mut self) -> f64 {
        self.0.borrow().mono
    }
    fn monotonic_ns(&mut self) -> i128 {
        (self.0.borrow().mono * 1e9) as i128
    }
    fn realtime_ns(&mut self) -> i128 {
        self.monotonic_ns() + self.0.borrow().offset
    }
    fn sleep(&mut self, seconds: f64) {
        self.0.borrow_mut().trace.push(json!(["sleep", seconds]));
    }
}
#[derive(Default)]
struct Output {
    packets: Vec<Value>,
    logs: Vec<Value>,
}
impl Sink for Output {
    fn send(
        &mut self,
        kind: Kind,
        event: &openpilot_sensord::sensor::Event,
        time: u64,
    ) -> Result<(), Error> {
        self.packets
            .push(support::packet(&wire::encode(kind, event, time)?)?);
        Ok(())
    }
    fn log(&mut self, level: &str, text: &str, _: Option<&Error>) -> Result<(), Error> {
        self.logs.push(json!([level, text]));
        Ok(())
    }
}
fn result<T: serde::Serialize>(result: Result<T, Error>) -> Value {
    match result {
        Ok(v) => json!({"result":v}),
        Err(Error::DataNotReady) => json!({"error":"DataNotReady"}),
        Err(Error::Io(_)) => json!({"error":"OSError"}),
        Err(Error::Contract("unexpected sensor chip ID")) => json!({"error":"AssertionError"}),
        Err(error) => json!({"error":error.to_string()}),
    }
}
fn main() -> Result<(), Error> {
    let request: Request = serde_json::from_reader(std::io::stdin())?;
    let mut registers = [0; 256];
    registers[0x0f] = request.chip;
    registers[0x0d] = 0x80;
    let state = Rc::new(RefCell::new(State {
        registers,
        trace: Vec::new(),
        mono: 1.,
        offset: 1_000_000_000,
        ready: 3,
        fault: None,
        fail_test: request.fail_self_test,
    }));
    let mut clock = FakeClock(Rc::clone(&state));
    let mut sensors: Vec<_> = [
        Kind::Accelerometer,
        Kind::Gyroscope,
        Kind::TemperatureSensor,
    ]
    .into_iter()
    .zip(0..3)
    .map(|(kind, index)| {
        Sensor::new(
            kind,
            FakeBus {
                index,
                state: Rc::clone(&state),
            },
        )
    })
    .collect();
    let mut rows = Vec::new();
    let mut output = Output::default();
    for sensor in &mut sensors {
        rows.push(result(sensor.reset(&mut clock)));
    }
    for sensor in &mut sensors {
        rows.push(result(
            sensor.init(&mut clock, request.self_test.as_deref()),
        ));
    }
    match request.mode.as_str() {
        "drivers" => {
            for sensor in &mut sensors {
                rows.push(result(
                    sensor
                        .get_event(&mut clock, Some(1234567890))
                        .and_then(|v| {
                            let bytes = wire::encode(sensor.kind, &v, 0)?;
                            Ok(support::packet(&bytes)?["event"].clone())
                        }),
                ));
            }
            for mono in [1., 1.5, 1.500001] {
                state.borrow_mut().mono = mono;
                rows.push(json!({"result":sensors.iter_mut().map(|sensor|sensor.valid(&mut clock)).collect::<Vec<_>>()}));
            }
        }
        "irq" => {
            let mut interrupt = Interrupt::new(&mut clock);
            for frame in request.frames {
                {
                    let mut s = state.borrow_mut();
                    s.mono = frame.mono;
                    s.offset = frame.offset;
                    s.ready = frame.ready;
                    s.fault = frame.fault;
                }
                let poll = match frame.poll.as_str() {
                    "timeout" => Poll::Timeout,
                    "other" => Poll::Other,
                    "short" => Poll::Data(vec![0; 4]),
                    _ => {
                        let mut data = frame.timestamp.to_ne_bytes().to_vec();
                        data.extend([0; 24]);
                        Poll::Data(data)
                    }
                };
                let mut borrowed = sensors.iter_mut().collect::<Vec<_>>();
                if let Err(error) = interrupt.step(poll, &mut borrowed, &mut clock, &mut output) {
                    rows.push(result::<()>(Err(error)));
                    break;
                }
            }
        }
        "poll" => {
            let mut rate = Ratekeeper::default();
            for frame in request.frames {
                {
                    let mut s = state.borrow_mut();
                    s.mono = frame.mono;
                    s.fault = frame.fault;
                }
                loops::polling_step(&mut sensors[2], &mut clock, &mut output, &mut rate)?;
            }
        }
        _ => return Err(Error::Contract("unknown trace mode")),
    }
    for sensor in &mut sensors {
        rows.push(result(sensor.shutdown()));
    }
    let s = state.borrow();
    serde_json::to_writer(
        std::io::stdout(),
        &json!({"rows":rows,"trace":s.trace,"logs":output.logs,"packets":output.packets,"registers":s.registers.to_vec()}),
    )?;
    Ok(())
}
