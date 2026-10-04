use crate::{
    diagnostics::{Diagnostics, ErrorEvent, RetryStats},
    priority::Arbiter,
    protocol::{AttemptTiming, BusTiming, Clock, Io, Protocol, Request, RequestError, CHUNK_SIZE},
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, OnceLock,
};

pub trait Transport: Io + Send {
    type Clock: Clock;
    fn clock(&self) -> Self::Clock;
    fn file_lock(&mut self);
    fn file_unlock(&mut self);
    fn yield_now(&mut self);
    fn sleep_us(&mut self, micros: u32);
    fn close(&mut self);
}
#[derive(Default)]
pub struct Shared {
    priority: Arbiter,
    bus: Mutex<BusTiming>,
    diagnostics: Diagnostics,
}
impl Shared {
    pub fn process() -> Arc<Self> {
        static BUS: OnceLock<Arc<Shared>> = OnceLock::new();
        Arc::clone(BUS.get_or_init(|| Arc::new(Shared::default())))
    }
    pub fn error_event(&self) -> ErrorEvent {
        self.diagnostics.event()
    }
    pub fn error_sequence(&self) -> u64 {
        self.diagnostics.sequence()
    }
}
struct State<T> {
    protocol: Protocol,
    io: T,
}
pub struct Device<T: Transport> {
    shared: Arc<Shared>,
    clock: T::Clock,
    state: Mutex<State<T>>,
    serial: String,
    connected: AtomicBool,
    healthy: AtomicBool,
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Request(#[from] RequestError),
    #[error("Error connecting to panda: failed to get serial")]
    SerialRead,
    #[error("Error connecting to panda: serial mismatch")]
    SerialMismatch,
    #[error("SPI bulk length exceeds the supported 16-bit domain")]
    BulkLength,
}
impl<T: Transport> Device<T> {
    /// Transport setup (mode 0, 50 MHz, 8 bits) must complete before this constructor.
    pub fn connect(io: T, shared: Arc<Shared>, expected_serial: &str) -> Result<Self, Error> {
        let clock = io.clock();
        let mut device = Self {
            shared,
            clock,
            state: Mutex::new(State {
                protocol: Protocol::default(),
                io,
            }),
            serial: String::new(),
            connected: AtomicBool::new(true),
            healthy: AtomicBool::new(true),
        };
        let mut uid = [0; 12];
        let result = device.control_read(0xc3, 0, 0, &mut uid, 100)?;
        if result != 12 {
            device
                .state
                .lock()
                .expect("SPI state mutex poisoned")
                .io
                .log(10, format!("failed to get serial {result}"));
            return Err(Error::SerialRead);
        }
        device.serial = uid.iter().map(|byte| format!("{byte:02x}")).collect();
        if !expected_serial.is_empty() && expected_serial != device.serial {
            return Err(Error::SerialMismatch);
        }
        Ok(device)
    }
    pub fn serial(&self) -> &str {
        &self.serial
    }
    pub fn connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }
    pub fn healthy(&self) -> bool {
        self.healthy.load(Ordering::SeqCst)
    }
    pub fn set_connected(&self, value: bool) {
        self.connected.store(value, Ordering::SeqCst);
    }
    fn control(request: u8, param1: u16, param2: u16, length: u16) -> [u8; 7] {
        let a = param1.to_le_bytes();
        let b = param2.to_le_bytes();
        let n = length.to_le_bytes();
        [request, a[0], a[1], b[0], b[1], n[0], n[1]]
    }
    pub fn control_write(
        &self,
        request: u8,
        param1: u16,
        param2: u16,
        timeout_ms: u32,
    ) -> Result<i32, Error> {
        let packet = Self::control(request, param1, param2, 0);
        self.retry(
            Request {
                endpoint: 0,
                data: Some(&packet),
                maximum: 0,
                timeout_ms,
            },
            None,
        )
    }
    pub fn control_read(
        &self,
        request: u8,
        param1: u16,
        param2: u16,
        output: &mut [u8],
        timeout_ms: u32,
    ) -> Result<i32, Error> {
        let maximum = u16::try_from(output.len())
            .map_err(|_| Error::Request(RequestError::PayloadTooLarge))?;
        let packet = Self::control(request, param1, param2, maximum);
        self.retry(
            Request {
                endpoint: 0,
                data: Some(&packet),
                maximum,
                timeout_ms,
            },
            Some(output),
        )
    }
    pub fn bulk_write(&self, endpoint: u8, input: &[u8], timeout_ms: u32) -> Result<i32, Error> {
        if input.len() > usize::from(u16::MAX) {
            return Err(Error::BulkLength);
        }
        let mut total: i32 = 0;
        for data in input.chunks(CHUNK_SIZE) {
            let result = self.retry(
                Request {
                    endpoint,
                    data: Some(data),
                    maximum: 0,
                    timeout_ms,
                },
                None,
            )?;
            if result < 0 {
                self.bulk_failed(result);
                return Ok(result);
            }
            total = total.wrapping_add(result);
        }
        Ok(total)
    }
    pub fn bulk_read(
        &self,
        endpoint: u8,
        output: &mut [u8],
        timeout_ms: u32,
    ) -> Result<i32, Error> {
        if output.len() > usize::from(u16::MAX) {
            return Err(Error::BulkLength);
        }
        let mut total: i32 = 0;
        let length = output.len();
        for start in (0..length).step_by(CHUNK_SIZE) {
            let maximum = CHUNK_SIZE.min(length - total as usize) as u16;
            let result = self.retry(
                Request {
                    endpoint,
                    data: None,
                    maximum,
                    timeout_ms,
                },
                Some(&mut output[start..start + usize::from(maximum)]),
            )?;
            if result < 0 {
                self.bulk_failed(result);
                return Ok(result);
            }
            total = total.wrapping_add(result);
            if result < CHUNK_SIZE as i32 {
                break;
            }
        }
        Ok(total)
    }
    fn bulk_failed(&self, result: i32) {
        let mut state = self.state.lock().expect("SPI state mutex poisoned");
        let State { protocol, io } = &mut *state;
        protocol.log(io, 40, format!("SPI: bulk transfer failed with {result}"));
        self.healthy.store(false, Ordering::SeqCst);
    }
    fn attempt(
        &self,
        request: Request<'_>,
        output: Option<&mut [u8]>,
    ) -> Result<(i32, AttemptTiming), Error> {
        let start = self.clock.now_ns();
        let _priority = self.shared.priority.acquire(request.endpoint);
        let mut bus = self.shared.bus.lock().expect("SPI hardware mutex poisoned");
        let mut state = self.state.lock().expect("SPI state mutex poisoned");
        let State { protocol, io } = &mut *state;
        io.file_lock();
        let locked = io.now_ns();
        let result = protocol.attempt(io, &mut bus, request, output, start, locked);
        io.file_unlock();
        Ok((result?, protocol.timing))
    }
    fn retry(&self, request: Request<'_>, mut output: Option<&mut [u8]>) -> Result<i32, Error> {
        let start_ms = self.clock.now_ms();
        let diag_start = self.clock.now_ns();
        let mut stats = RetryStats::default();
        let mut timeout_count: i32 = 0;
        let mut nack_count: i32 = 0;
        let result = loop {
            let (result, timing) = self.attempt(request, output.as_deref_mut())?;
            stats.observe(result, timing);
            let mut timed_out = false;
            if result < 0 {
                timed_out = request.timeout_ms != 0 && timeout_count > 5;
                timeout_count = timeout_count.wrapping_add(i32::from(result == -3));
                let mut state = self.state.lock().expect("SPI state mutex poisoned");
                let State { protocol, io } = &mut *state;
                io.yield_now();
                if result == -2 {
                    nack_count = nack_count.wrapping_add(1);
                    if nack_count > 3 {
                        protocol.log(io, 10, format!("NACK sleep {nack_count}"));
                        io.sleep_us(nack_count.saturating_mul(10).clamp(200, 2000) as u32);
                    }
                }
            }
            if result >= 0 || !self.connected() || timed_out {
                break result;
            }
        };
        let mut state = self.state.lock().expect("SPI state mutex poisoned");
        let State { protocol, io } = &mut *state;
        let total_us = io.now_ns().wrapping_sub(diag_start) / 1000;
        for message in self
            .shared
            .diagnostics
            .record(request, result, total_us, &stats)
        {
            io.log(30, message);
        }
        if result < 0 {
            let elapsed = io.now_ms() - start_ms;
            protocol.log(
                io,
                40,
                format!("transfer failed, after {timeout_count} tries, {elapsed:.2}ms"),
            );
        }
        Ok(result)
    }
}
impl<T: Transport> Drop for Device<T> {
    fn drop(&mut self) {
        let _bus = self.shared.bus.lock().expect("SPI hardware mutex poisoned");
        self.state
            .get_mut()
            .expect("SPI state mutex poisoned")
            .io
            .close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{atomic::AtomicU64, Condvar};
    use std::time::{Duration, Instant};
    #[derive(Clone)]
    struct TestClock(Arc<AtomicU64>);
    impl Clock for TestClock {
        fn now_ns(&self) -> u64 {
            self.0.fetch_add(100_000, Ordering::SeqCst) + 100_000
        }
    }
    #[derive(Default)]
    struct GateState {
        entered: bool,
        released: bool,
        order: Vec<u8>,
    }
    #[derive(Default)]
    struct Gate {
        state: Mutex<GateState>,
        changed: Condvar,
    }
    struct BlockingIo {
        clock: TestClock,
        gate: Arc<Gate>,
        phase: u8,
    }
    impl Io for BlockingIo {
        fn now_ns(&mut self) -> u64 {
            self.clock.now_ns()
        }
        fn log(&mut self, _: u8, _: String) {}
        fn transfer(&mut self, tx: &mut [u8], rx: &mut [u8], length: usize) -> i32 {
            rx[..length].fill(0);
            match self.phase {
                0 => {
                    assert_eq!(tx[0], 0x5a);
                    let mut state = self.gate.state.lock().unwrap();
                    state.order.push(tx[1]);
                    if !state.entered {
                        state.entered = true;
                        self.gate.changed.notify_all();
                        while !state.released {
                            state = self.gate.changed.wait(state).unwrap();
                        }
                    }
                }
                1 => rx[0] = 0x79,
                2 => {}
                3 => rx[0] = 0x85,
                4 => rx[0] = 0x2e,
                _ => unreachable!(),
            }
            self.phase = (self.phase + 1) % 5;
            length as i32
        }
    }
    impl Transport for BlockingIo {
        type Clock = TestClock;
        fn clock(&self) -> Self::Clock {
            self.clock.clone()
        }
        fn file_lock(&mut self) {}
        fn file_unlock(&mut self) {}
        fn yield_now(&mut self) {
            std::thread::yield_now();
        }
        fn sleep_us(&mut self, _: u32) {}
        fn close(&mut self) {}
    }
    #[test]
    fn same_handle_waiters_register_before_current_transfer_releases_buffers() {
        let shared = Arc::new(Shared::default());
        let clock = TestClock(Arc::new(AtomicU64::new(1_000_000_000)));
        let gate = Arc::new(Gate::default());
        let device = Arc::new(Device {
            shared: Arc::clone(&shared),
            clock: clock.clone(),
            state: Mutex::new(State {
                protocol: Protocol::default(),
                io: BlockingIo {
                    clock,
                    gate: Arc::clone(&gate),
                    phase: 0,
                },
            }),
            serial: "fixture".into(),
            connected: AtomicBool::new(true),
            healthy: AtomicBool::new(true),
        });
        let control = Arc::clone(&device);
        let control = std::thread::spawn(move || control.control_write(0xa1, 0, 0, 1).unwrap());
        {
            let mut state = gate.state.lock().unwrap();
            while !state.entered {
                state = gate.changed.wait(state).unwrap();
            }
        }
        let tx = Arc::clone(&device);
        let tx = std::thread::spawn(move || tx.bulk_write(3, &[1], 1).unwrap());
        let rx = Arc::clone(&device);
        let rx = std::thread::spawn(move || rx.bulk_read(0x81, &mut [0; 1], 1).unwrap());
        let deadline = Instant::now() + Duration::from_secs(5);
        while shared.priority.waiters() != [0, 1, 1] && Instant::now() < deadline {
            std::thread::yield_now();
        }
        let queued = shared.priority.waiters();
        gate.state.lock().unwrap().released = true;
        gate.changed.notify_all();
        assert_eq!(control.join().unwrap(), 0);
        assert_eq!(tx.join().unwrap(), 0);
        assert_eq!(rx.join().unwrap(), 0);
        assert_eq!(
            queued,
            [0, 1, 1],
            "clock sampling must not wait for the handle's active transfer"
        );
        assert_eq!(gate.state.lock().unwrap().order, [0, 3, 0x81]);
    }
}
