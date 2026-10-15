//! Actual-source action comparison; no GPU or process launch in this trace.
use openpilot_signal_observer::{
    auto::{self, Error, Owner, Runtime},
    backend::{Backend, Request},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    io::{self, Read},
    rc::Rc,
    time::Duration,
};
#[derive(Deserialize)]
struct Case {
    modes: Vec<Backend>,
    stubborn: bool,
    fault: bool,
    read_error: bool,
}
struct Child {
    pid: u32,
    alive: bool,
    stubborn: bool,
    fault: bool,
    tick: Rc<Cell<usize>>,
    actions: Rc<RefCell<Vec<Value>>>,
}
impl Child {
    fn action(&self, value: Value) {
        self.actions.borrow_mut().push(value);
    }
}
impl Owner for Child {
    fn pid(&self) -> u32 {
        self.pid
    }
    fn is_alive(&mut self) -> Result<bool, Error> {
        self.action(json!(["alive", self.pid, self.alive]));
        Ok(self.alive)
    }
    fn terminate(&mut self) -> Result<(), Error> {
        self.action(json!(["terminate", self.pid]));
        if !self.stubborn {
            self.alive = false;
        }
        Ok(())
    }
    fn join(&mut self, timeout: Duration) -> Result<(), Error> {
        self.action(json!(["join", self.pid, timeout.as_secs()]));
        Ok(())
    }
    fn kill(&mut self) -> Result<(), Error> {
        self.action(json!(["kill", self.pid]));
        self.alive = false;
        Ok(())
    }
    fn exit_code(&mut self) -> Result<Option<i32>, Error> {
        let code = (self.fault && self.tick.get() == 1).then_some(7);
        self.action(json!(["exitcode", self.pid, code]));
        Ok(code)
    }
    fn close(self) {
        self.action(json!(["close", self.pid]));
    }
}
struct Io {
    case: Case,
    tick: Rc<Cell<usize>>,
    created: u32,
    actions: Rc<RefCell<Vec<Value>>>,
}
impl Io {
    fn action(&self, value: Value) {
        self.actions.borrow_mut().push(value);
    }
}
impl Runtime for Io {
    type Child = Child;
    fn selection(&mut self) -> Result<Request, Error> {
        self.action(json!(["selection", "auto"]));
        Ok(Request::Auto)
    }
    fn requested(&mut self) -> Result<bool, Error> {
        let value = self.tick.get() < self.case.modes.len();
        self.action(json!(["requested", value]));
        Ok(value)
    }
    fn neural_requested(&mut self) -> Result<bool, Error> {
        self.action(json!(["neural", true]));
        Ok(true)
    }
    fn automatic_backend(&mut self) -> Result<Backend, Error> {
        if self.case.read_error && self.tick.get() == 1 {
            self.action(json!(["automatic_error"]));
            return Err("source params read failed".into());
        }
        let backend = self.case.modes[self.tick.get()];
        self.action(json!(["automatic", backend]));
        Ok(backend)
    }
    fn start(&mut self, backend: Backend) -> Result<Child, Error> {
        let pid = 100 + self.created;
        self.created += 1;
        self.action(json!(["create", backend, pid]));
        self.action(json!(["start", pid]));
        Ok(Child {
            pid,
            alive: true,
            stubborn: self.case.stubborn,
            fault: self.case.fault,
            tick: self.tick.clone(),
            actions: self.actions.clone(),
        })
    }
    fn event(&mut self, backend: Backend, pid: u32) -> Result<(), Error> {
        self.action(json!(["event","signalAutoBackend",{"backend":backend,"pid":pid,"control_permission":false}]));
        Ok(())
    }
    fn now(&mut self) -> f64 {
        self.tick.get() as f64 * 0.1
    }
    fn wait(&mut self, duration: Duration) {
        self.action(json!(["sleep", duration.as_secs_f64()]));
        self.tick.set(self.tick.get() + 1);
    }
    fn interrupted(&self) -> bool {
        false
    }
}
fn main() -> Result<(), Error> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let output = cases
        .into_iter()
        .map(|case| {
            let mut io = Io {
                case,
                tick: Rc::new(Cell::new(0)),
                created: 0,
                actions: Rc::new(RefCell::new(vec![])),
            };
            if let Err(error) = auto::run(&mut io, None) {
                io.action(json!(["error", error.to_string()]));
            }
            let actions = io.actions.borrow().clone();
            actions
        })
        .collect::<Vec<_>>();
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
