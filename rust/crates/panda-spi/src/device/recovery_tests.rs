use super::*;
use std::sync::atomic::AtomicU64;

#[derive(Clone)]
struct TestClock(Arc<AtomicU64>);
impl Clock for TestClock {
    fn now_ns(&self) -> u64 {
        self.0.fetch_add(1_000_000, Ordering::SeqCst) + 1_000_000
    }
}

#[derive(Clone, Copy)]
enum Fault {
    None,
    Io,
    Silent,
    Cancel,
    Recover,
}
type Cancel = Arc<Mutex<Option<Box<dyn Fn() + Send>>>>;
struct TestIo {
    clock: TestClock,
    fault: Fault,
    phase: u8,
    calls: usize,
    recovery_calls: usize,
    cancelled: Cancel,
    locked: bool,
    escaped: bool,
}
impl Io for TestIo {
    fn now_ns(&mut self) -> u64 {
        self.clock.now_ns()
    }
    fn log(&mut self, _: u8, _: String) {}
    fn transfer(&mut self, tx: &mut [u8], rx: &mut [u8], length: usize) -> i32 {
        assert!(self.locked);
        self.calls += 1;
        rx[..length].fill(0);
        if !matches!(self.fault, Fault::None) {
            if length == 7 {
                assert_eq!(tx[0], 0x5a);
                return -1;
            }
            assert_eq!(length, 1024);
            assert!(tx[..length].iter().all(|byte| *byte == 0x14));
            self.recovery_calls += 1;
            // Let the unfixed implementation escape instead of hanging the test process forever.
            if self.recovery_calls > 1000 {
                self.escaped = true;
                rx[0] = 0x1f;
                if self.recovery_calls == 1003 {
                    self.fault = Fault::None;
                }
                return length as i32;
            }
            match self.fault {
                Fault::Io => return -1,
                Fault::Silent => return length as i32,
                Fault::Cancel => {
                    if self.recovery_calls == 1 {
                        self.cancelled.lock().unwrap().as_ref().unwrap()();
                    }
                }
                Fault::Recover => {
                    if self.recovery_calls == 3 {
                        self.fault = Fault::None;
                    }
                }
                Fault::None => unreachable!(),
            }
            rx[0] = 0x1f;
            return length as i32;
        }
        match self.phase {
            0 => assert_eq!(tx[0], 0x5a),
            1 => rx[0] = 0x79,
            2 => (),
            3 => rx[..3].copy_from_slice(&[0x85, 1, 0]),
            4 => rx[..2].copy_from_slice(&[0x31, 0x1e]),
            _ => unreachable!(),
        }
        self.phase = (self.phase + 1) % 5;
        length as i32
    }
}
impl Transport for TestIo {
    type Clock = TestClock;
    fn clock(&self) -> TestClock {
        self.clock.clone()
    }
    fn file_lock(&mut self) {
        assert!(!self.locked);
        self.locked = true;
    }
    fn file_unlock(&mut self) {
        assert!(self.locked);
        self.locked = false;
    }
    fn yield_now(&mut self) {}
    fn sleep_us(&mut self, micros: u32) {
        self.clock
            .0
            .fetch_add(u64::from(micros) * 1000, Ordering::SeqCst);
    }
    fn close(&mut self) {
        assert!(!self.locked);
    }
}
fn device(fault: Fault) -> Arc<Device<TestIo>> {
    let clock = TestClock(Arc::new(AtomicU64::new(1_000_000_000)));
    let cancelled: Cancel = Arc::default();
    let device = Arc::new(Device {
        shared: Arc::default(),
        clock: clock.clone(),
        state: Mutex::new(State {
            protocol: Protocol::default(),
            io: TestIo {
                clock,
                fault,
                phase: 0,
                calls: 0,
                recovery_calls: 0,
                cancelled: Arc::clone(&cancelled),
                locked: false,
                escaped: false,
            },
        }),
        serial: "fixture".into(),
        connected: AtomicBool::new(true),
        healthy: AtomicBool::new(true),
    });
    let weak = Arc::downgrade(&device);
    *cancelled.lock().unwrap() = Some(Box::new(move || {
        weak.upgrade().unwrap().set_connected(false)
    }));
    device
}

#[test]
fn permanent_recovery_errors_and_missing_ack_terminate_even_with_zero_timeout() {
    for fault in [Fault::Io, Fault::Silent] {
        for timeout in [0, 1, 100, u32::MAX] {
            let device = device(fault);
            let mut bytes = [0xa5];
            let result = device.bulk_read(0x81, &mut bytes, timeout);
            assert!(
                matches!(result, Err(Error::Request(RequestError::RecoveryTimeout))),
                "failed recovery must return a terminal timeout: {result:?}"
            );
            assert!(!device.connected());
            assert!(!device.healthy());
            assert_eq!(bytes, [0xa5]);
            let calls = {
                let state = device.state.try_lock().unwrap();
                assert!(!state.io.locked);
                assert!(!state.io.escaped, "recovery exceeded the fixture watchdog");
                assert_eq!(
                    state.protocol.timing.failure_phase,
                    crate::protocol::FailurePhase::HeaderIo
                );
                assert!(state.protocol.timing.recovery_us >= 500_000);
                assert!(state.protocol.timing.recovery_us <= 525_000);
                state.io.calls
            };
            assert!(device.shared.bus.try_lock().is_ok());
            let priority = device.shared.priority.acquire(0x81);
            drop(priority);
            assert!(device.control_write(0xa1, 0, 0, 100).is_err());
            assert_eq!(
                device.state.lock().unwrap().io.calls,
                calls,
                "disconnected device must not send another header on an unrecovered bus"
            );
        }
    }
}

#[test]
fn cancellation_during_recovery_releases_the_bus_without_more_transfers() {
    let device = device(Fault::Cancel);
    let mut bytes = [0xa5];
    let result = device.bulk_read(0x81, &mut bytes, 0);
    assert!(
        matches!(result, Err(Error::Request(RequestError::Cancelled))),
        "cancellation must be terminal: {result:?}"
    );
    assert!(!device.connected());
    assert!(!device.healthy());
    let state = device.state.try_lock().unwrap();
    assert!(!state.io.locked);
    assert_eq!(state.io.calls, 2);
    assert_eq!(bytes, [0xa5]);
    assert!(device.shared.bus.try_lock().is_ok());
    let _priority = device.shared.priority.acquire(3);
}

#[test]
fn zero_timeout_preserves_success_and_completed_recovery_retry() {
    for fault in [Fault::None, Fault::Recover] {
        let device = device(fault);
        let mut bytes = [0xa5];
        assert_eq!(device.bulk_read(0x81, &mut bytes, 0).unwrap(), 1);
        assert_eq!(bytes, [0x31]);
        assert!(device.connected());
        assert!(device.healthy());
        let state = device.state.lock().unwrap();
        assert_eq!(
            state.io.calls,
            if matches!(fault, Fault::None) { 5 } else { 9 }
        );
        assert!(!state.io.locked);
    }
}
