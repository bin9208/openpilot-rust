use openpilot_amplifier::{Amplifier, Bus, Platform};
use std::{cell::RefCell, io, rc::Rc};

#[derive(Default)]
struct State {
    register: u8,
    writes: Vec<(u8, u8)>,
    opens: usize,
    closes: usize,
    fail_reads: usize,
    sleeps: Vec<f64>,
    output: Vec<String>,
}

struct Fixture(Rc<RefCell<State>>);
struct Device(Rc<RefCell<State>>);

impl Bus for Device {
    fn read_byte(&mut self, register: u8) -> io::Result<u8> {
        assert_eq!(register, 0x51);
        let mut state = self.0.borrow_mut();
        if state.fail_reads > 0 {
            state.fail_reads -= 1;
            return Err(io::Error::from_raw_os_error(5));
        }
        Ok(state.register)
    }

    fn write_byte(&mut self, register: u8, value: u8) -> io::Result<()> {
        self.0.borrow_mut().writes.push((register, value));
        Ok(())
    }

    fn close(self) -> io::Result<()> {
        self.0.borrow_mut().closes += 1;
        Ok(())
    }
}

impl Platform for Fixture {
    type Bus = Device;
    fn open_bus(&mut self) -> io::Result<Device> {
        self.0.borrow_mut().opens += 1;
        Ok(Device(Rc::clone(&self.0)))
    }
    fn sleep(&mut self, seconds: f64) -> io::Result<()> {
        self.0.borrow_mut().sleeps.push(seconds);
        Ok(())
    }
    fn print(&mut self, text: &str) -> io::Result<()> {
        self.0.borrow_mut().output.push(text.into());
        Ok(())
    }
}

#[test]
fn shutdown_retains_other_bits_after_retry() {
    let state = Rc::new(RefCell::new(State {
        register: 0xd5,
        fail_reads: 1,
        ..State::default()
    }));
    let mut platform = Fixture(Rc::clone(&state));
    let result = Amplifier::new(false)
        .set_global_shutdown(&mut platform, true)
        .unwrap();
    let state = state.borrow();
    assert!(result);
    assert_eq!(state.writes, [(0x51, 0x55)]);
    assert_eq!((state.opens, state.closes), (2, 2));
    assert_eq!(state.sleeps, [0.1]);
    assert_eq!(state.output, ["Failed to set amp config, 14 retries left"]);
}

#[test]
fn exhausted_bus_errors_return_false_after_final_backoff() {
    let state = Rc::new(RefCell::new(State {
        fail_reads: 15,
        ..State::default()
    }));
    let mut platform = Fixture(Rc::clone(&state));
    let result = Amplifier::new(false)
        .set_global_shutdown(&mut platform, false)
        .unwrap();
    let state = state.borrow();
    assert!(!result);
    assert!(state.writes.is_empty());
    assert_eq!((state.opens, state.closes), (15, 15));
    assert_eq!(state.sleeps.len(), 15);
    assert!((state.sleeps[14] - 1.5).abs() < 1e-14);
    assert_eq!(
        state.output.last().map(String::as_str),
        Some("Failed to set amp config, 0 retries left")
    );
}
