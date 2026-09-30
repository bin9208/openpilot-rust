use openpilot_amplifier::{Amplifier, Bus, Error, Platform};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::BTreeSet,
    io::{self, BufRead},
    rc::Rc,
};

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum Operation {
    Initialize { model: String },
    Shutdown { disabled: bool },
}
#[derive(Deserialize)]
struct Input {
    #[serde(flatten)]
    operation: Operation,
    debug: bool,
    registers: Vec<u8>,
    #[serde(default)]
    fail_events: BTreeSet<usize>,
    #[serde(default)]
    fail_kinds: BTreeSet<String>,
}
struct State {
    registers: Vec<u8>,
    events: Vec<Value>,
    fail_events: BTreeSet<usize>,
    fail_kinds: BTreeSet<String>,
}
impl State {
    fn record(&mut self, event: Value) -> io::Result<()> {
        let kind = event["kind"]
            .as_str()
            .ok_or_else(|| io::Error::other("missing event kind"))?
            .to_owned();
        let index = self.events.len();
        self.events.push(event);
        if self.fail_events.contains(&index) || self.fail_kinds.contains(&kind) {
            Err(io::Error::other(format!("{kind}@{index}")))
        } else {
            Ok(())
        }
    }
}
struct Fixture(Rc<RefCell<State>>);
struct Device(Rc<RefCell<State>>);
impl Bus for Device {
    fn read_byte(&mut self, register: u8) -> io::Result<u8> {
        let mut state = self.0.borrow_mut();
        state.record(json!({"kind":"read", "register":register}))?;
        Ok(state.registers[usize::from(register)])
    }
    fn write_byte(&mut self, register: u8, value: u8) -> io::Result<()> {
        let mut state = self.0.borrow_mut();
        state.record(json!({"kind":"write", "register":register,"value":value}))?;
        state.registers[usize::from(register)] = value;
        Ok(())
    }
    fn close(self) -> io::Result<()> {
        self.0.borrow_mut().record(json!({"kind":"close"}))
    }
}
impl Platform for Fixture {
    type Bus = Device;
    fn open_bus(&mut self) -> io::Result<Device> {
        self.0.borrow_mut().record(json!({"kind":"open"}))?;
        Ok(Device(Rc::clone(&self.0)))
    }
    fn sleep(&mut self, seconds: f64) -> io::Result<()> {
        self.0
            .borrow_mut()
            .record(json!({"kind":"sleep","seconds":seconds}))
    }
    fn print(&mut self, text: &str) -> io::Result<()> {
        self.0
            .borrow_mut()
            .record(json!({"kind":"print","text":text}))
    }
}
fn run(input: Input) -> Result<Value, Box<dyn std::error::Error>> {
    if input.registers.len() != 256 {
        return Err("expected256 register bytes".into());
    }
    let state = Rc::new(RefCell::new(State {
        registers: input.registers,
        events: Vec::new(),
        fail_events: input.fail_events,
        fail_kinds: input.fail_kinds,
    }));
    let mut fixture = Fixture(Rc::clone(&state));
    let amplifier = Amplifier::new(input.debug);
    let result = match input.operation {
        Operation::Initialize { model } => amplifier.initialize_configuration(&mut fixture, &model),
        Operation::Shutdown { disabled } => amplifier.set_global_shutdown(&mut fixture, disabled),
    };
    let outcome = match result {
        Ok(value) => json!({"value":value}),
        Err(Error::UnknownModel(_)) => json!({"error":"unknown_model"}),
        Err(Error::Io(error)) => json!({"error":"io","detail":error.to_string()}),
    };
    let state = state.borrow();
    Ok(json!({"outcome":outcome,"events":state.events,"registers":state.registers}))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        println!("{}", run(serde_json::from_str(&line?)?)?);
    }
    Ok(())
}
