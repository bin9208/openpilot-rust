//! Transaction framing and recovery from selfdrive/pandad/spi.cc.
use serde::{Deserialize, Serialize};

pub const BUFFER_SIZE: usize = 2048;
pub const CHUNK_SIZE: usize = BUFFER_SIZE - 0x40;
pub const CAN_TURNAROUND_NS: u64 = 400_000;
pub const CONTROL_TURNAROUND_NS: u64 = 1_000_000;
const NACK: i32 = -2;
const ACK_TIMEOUT: i32 = -3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailurePhase {
    #[default]
    None,
    HeaderIo,
    HackNack,
    HackTimeout,
    DataIo,
    DackNack,
    DackTimeout,
    RxLength,
    RxIo,
    RxChecksum,
}
impl FailurePhase {
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::HeaderIo => "header_io",
            Self::HackNack => "hack_nack",
            Self::HackTimeout => "hack_timeout",
            Self::DataIo => "data_io",
            Self::DackNack => "dack_nack",
            Self::DackTimeout => "dack_timeout",
            Self::RxLength => "rx_length",
            Self::RxIo => "rx_io",
            Self::RxChecksum => "rx_checksum",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptTiming {
    pub lock_us: u64,
    pub turnaround_us: u64,
    pub hack_us: u64,
    pub dack_us: u64,
    pub recovery_us: u64,
    pub total_us: u64,
    pub recovery_restarts: u32,
    pub failure_phase: FailurePhase,
}
#[derive(Debug)]
pub struct BusTiming {
    pub last_activity_ns: u64,
    pub next_transaction_ns: u64,
}
impl Default for BusTiming {
    fn default() -> Self {
        Self {
            last_activity_ns: 0,
            next_transaction_ns: CAN_TURNAROUND_NS,
        }
    }
}

/// Clock access is independent of the handle mutex so waiting CAN threads register
/// their priority even while another operation owns that handle's buffers.
pub trait Clock: Send + Sync {
    fn now_ns(&self) -> u64;
    fn now_ms(&self) -> f64 {
        let ns = self.now_ns();
        (ns / 1_000_000_000) as f64 * 1000.0 + (ns % 1_000_000_000) as f64 * 1e-6
    }
}

/// Synchronous transport. Implementations must fill only the supplied receive slice;
/// a negative return is the original ioctl result, including EINTR retries below this boundary.
pub trait Io {
    fn now_ns(&mut self) -> u64;
    fn transfer(&mut self, tx: &mut [u8], rx: &mut [u8], length: usize) -> i32;
    fn log(&mut self, level: u8, message: String);
    fn now_ms(&mut self) -> f64 {
        let ns = self.now_ns();
        (ns / 1_000_000_000) as f64 * 1000.0 + (ns % 1_000_000_000) as f64 * 1e-6
    }
}

#[derive(Clone, Copy)]
pub struct Request<'a> {
    pub endpoint: u8,
    pub data: Option<&'a [u8]>,
    pub maximum: u16,
    pub timeout_ms: u32,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RequestError {
    #[error("SPI payload must leave room for its checksum")]
    PayloadTooLarge,
    #[error("SPI caller buffer is smaller than its requested response")]
    OutputTooSmall,
}

pub struct Protocol {
    tx: [u8; BUFFER_SIZE],
    rx: [u8; BUFFER_SIZE],
    header: [u8; 6],
    pub transfer_count: u32,
    pub timing: AttemptTiming,
}
impl Default for Protocol {
    fn default() -> Self {
        Self {
            tx: [0; BUFFER_SIZE],
            rx: [0; BUFFER_SIZE],
            header: [0; 6],
            transfer_count: 0,
            timing: AttemptTiming::default(),
        }
    }
}
fn elapsed_us(now: u64, start: u64) -> u64 {
    now.wrapping_sub(start) / 1000
}
fn wait(io: &mut impl Io, start: u64, duration: u64) {
    while io.now_ns().wrapping_sub(start) < duration {}
}
fn checksum(data: &[u8]) -> u8 {
    data.iter().fold(0xab, |sum, byte| sum ^ byte)
}
impl Protocol {
    pub(crate) fn log(&self, io: &mut impl Io, level: u8, message: String) {
        io.log(level, message);
        let length = usize::from(u16::from_le_bytes([self.header[2], self.header[3]]));
        let prefix: String = self.tx[..length.min(8)]
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect();
        io.log(
            level,
            format!(
                "  {} / 0x{:x} / {} / {} / tx: {}",
                self.transfer_count,
                self.header[1],
                length,
                u16::from_le_bytes([self.header[4], self.header[5]]),
                prefix
            ),
        );
    }
    fn ack(&mut self, io: &mut impl Io, ack: u8, tx: u8, timeout: u32, length: usize) -> i32 {
        let start = io.now_ms();
        let timeout = if timeout == 0 { 500 } else { timeout }.clamp(20, 500);
        self.tx[..length].fill(tx);
        loop {
            let result = io.transfer(&mut self.tx, &mut self.rx, length);
            if result < 0 {
                self.log(io, 40, "SPI: failed to send ACK request".into());
                return result;
            }
            if self.rx[0] == ack {
                return 0;
            }
            if self.rx[0] == 0x1f {
                self.log(io, 10, format!("SPI: got NACK, waiting for 0x{ack:x}"));
                return NACK;
            }
            if io.now_ms() - start > f64::from(timeout) {
                self.log(
                    io,
                    30,
                    format!("SPI: timed out waiting for ACK, waiting for 0x{ack:x}"),
                );
                return ACK_TIMEOUT;
            }
        }
    }

    /// Executes with the caller already holding priority, process bus and file locks.
    /// `attempt_start_ns` is sampled before those locks; `locked_ns` immediately after.
    pub fn attempt(
        &mut self,
        io: &mut impl Io,
        bus: &mut BusTiming,
        request: Request<'_>,
        output: Option<&mut [u8]>,
        attempt_start_ns: u64,
        locked_ns: u64,
    ) -> Result<i32, RequestError> {
        let tx_length = request.data.map_or(0, <[u8]>::len);
        if tx_length >= BUFFER_SIZE || usize::from(request.maximum) >= BUFFER_SIZE {
            return Err(RequestError::PayloadTooLarge);
        }
        if output
            .as_ref()
            .is_some_and(|data| data.len() < usize::from(request.maximum))
        {
            return Err(RequestError::OutputTooSmall);
        }
        self.timing = AttemptTiming {
            lock_us: elapsed_us(locked_ns, attempt_start_ns),
            ..AttemptTiming::default()
        };
        let safety_control = request.endpoint == 0
            && request
                .data
                .is_some_and(|data| data.len() >= 7 && data[0] == 0xdc);
        let phase_start = io.now_ns();
        let turnaround = if request.endpoint == 0 {
            CONTROL_TURNAROUND_NS
        } else {
            CAN_TURNAROUND_NS
        };
        wait(
            io,
            bus.last_activity_ns,
            bus.next_transaction_ns.max(turnaround),
        );
        self.timing.turnaround_us = elapsed_us(io.now_ns(), phase_start);
        self.transfer_count = self.transfer_count.wrapping_add(1);
        let length = (tx_length as u16).to_le_bytes();
        let maximum = request.maximum.to_le_bytes();
        self.header = [
            0x5a,
            request.endpoint,
            length[0],
            length[1],
            maximum[0],
            maximum[1],
        ];
        self.tx[..6].copy_from_slice(&self.header);
        self.tx[6] = checksum(&self.header);
        let result = self.exchange(io, request, output, tx_length);
        if result >= 0 {
            bus.last_activity_ns = io.now_ns();
            bus.next_transaction_ns = if safety_control {
                CONTROL_TURNAROUND_NS
            } else {
                CAN_TURNAROUND_NS
            };
            self.timing.total_us = elapsed_us(bus.last_activity_ns, attempt_start_ns);
            return Ok(result);
        }
        let phase_start = io.now_ns();
        let mut nacks = 0;
        while nacks < 3 {
            if self.ack(io, 0x1f, 0x14, 1, BUFFER_SIZE / 2) == 0 {
                nacks += 1;
            } else {
                nacks = 0;
                self.timing.recovery_restarts = self.timing.recovery_restarts.wrapping_add(1);
            }
        }
        self.timing.recovery_us = elapsed_us(io.now_ns(), phase_start);
        bus.last_activity_ns = io.now_ns();
        bus.next_transaction_ns = CAN_TURNAROUND_NS;
        self.timing.total_us = elapsed_us(bus.last_activity_ns, attempt_start_ns);
        Ok(result)
    }
    fn exchange(
        &mut self,
        io: &mut impl Io,
        request: Request<'_>,
        output: Option<&mut [u8]>,
        tx_length: usize,
    ) -> i32 {
        let result = io.transfer(&mut self.tx, &mut self.rx, 7);
        if result < 0 {
            self.timing.failure_phase = FailurePhase::HeaderIo;
            self.log(io, 40, "SPI: failed to send header".into());
            return result;
        }
        let phase_start = io.now_ns();
        let result = self.ack(io, 0x79, 0x11, request.timeout_ms, 1);
        self.timing.hack_us = elapsed_us(io.now_ns(), phase_start);
        if result < 0 {
            self.timing.failure_phase = match result {
                NACK => FailurePhase::HackNack,
                ACK_TIMEOUT => FailurePhase::HackTimeout,
                _ => FailurePhase::HeaderIo,
            };
            return result;
        }
        let phase_start = io.now_ns();
        let turnaround_start = io.now_ns();
        wait(io, turnaround_start, CAN_TURNAROUND_NS);
        self.timing.turnaround_us += elapsed_us(io.now_ns(), phase_start);
        if let Some(data) = request.data {
            self.tx[..tx_length].copy_from_slice(data);
        }
        self.tx[tx_length] = checksum(&self.tx[..tx_length]);
        let result = io.transfer(&mut self.tx, &mut self.rx, tx_length + 1);
        if result < 0 {
            self.timing.failure_phase = FailurePhase::DataIo;
            self.log(io, 40, "SPI: failed to send data".into());
            return result;
        }
        let phase_start = io.now_ns();
        let result = self.ack(io, 0x85, 0x13, request.timeout_ms, 3);
        self.timing.dack_us = elapsed_us(io.now_ns(), phase_start);
        if result < 0 {
            self.timing.failure_phase = match result {
                NACK => FailurePhase::DackNack,
                ACK_TIMEOUT => FailurePhase::DackTimeout,
                _ => FailurePhase::DataIo,
            };
            return result;
        }
        let length = usize::from(u16::from_le_bytes([self.rx[1], self.rx[2]]));
        // #179: source permits oversized copies and an unaligned u16 load. Preserve valid frames only.
        if length + 4 > BUFFER_SIZE
            || output
                .as_ref()
                .is_some_and(|data| length > data.len() || length > usize::from(request.maximum))
        {
            self.timing.failure_phase = FailurePhase::RxLength;
            self.log(
                io,
                40,
                format!("SPI: RX data len larger than buf size {length}"),
            );
            return -1;
        }
        let result = io.transfer(&mut self.tx, &mut self.rx[3..], length + 1);
        if result < 0 {
            self.timing.failure_phase = FailurePhase::RxIo;
            self.log(io, 40, "SPI: failed to read rx data".into());
            return result;
        }
        if checksum(&self.rx[..length + 4]) != 0 {
            self.timing.failure_phase = FailurePhase::RxChecksum;
            self.log(io, 40, "SPI: bad checksum".into());
            return -1;
        }
        if let Some(data) = output {
            data[..length].copy_from_slice(&self.rx[3..length + 3]);
        }
        length as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    struct Script {
        steps: VecDeque<(Vec<u8>, Vec<u8>, i32)>,
        logs: Vec<String>,
        now: u64,
    }
    impl Io for Script {
        fn now_ns(&mut self) -> u64 {
            self.now += 100_000;
            self.now
        }
        fn transfer(&mut self, tx: &mut [u8], rx: &mut [u8], length: usize) -> i32 {
            let tx = &tx[..length];
            let rx = &mut rx[..length];
            let (expected, reply, result) = self.steps.pop_front().expect("unexpected transfer");
            assert_eq!(tx, expected);
            assert_eq!(rx.len(), reply.len());
            rx.copy_from_slice(&reply);
            result
        }
        fn log(&mut self, _: u8, message: String) {
            self.logs.push(message);
        }
    }
    fn script(length: u16, response: &[u8]) -> Script {
        let header = vec![0x5a, 0, 7, 0, 1, 0, 0xf7];
        let payload = vec![0xa1, 0, 0, 0, 0, 1, 0, 0x0b];
        let [lo, hi] = length.to_le_bytes();
        Script {
            steps: VecDeque::from([
                (header, vec![0; 7], 7),
                (vec![0x11], vec![0x79], 1),
                (payload, vec![0; 8], 8),
                (vec![0x13; 3], vec![0x85, lo, hi], 3),
                (
                    vec![0x13; response.len()],
                    response.to_vec(),
                    response.len() as i32,
                ),
            ]),
            logs: vec![],
            now: 1_000_000_000,
        }
    }
    #[test]
    fn valid_control_frame_matches_source_protocol_bytes() {
        let mut io = script(1, &[0x31, 0x1e]);
        let mut state = Protocol::default();
        let mut bus = BusTiming::default();
        let mut output = [0xa5];
        let data = [0xa1, 0, 0, 0, 0, 1, 0];
        let request = Request {
            endpoint: 0,
            data: Some(&data),
            maximum: 1,
            timeout_ms: 100,
        };
        assert_eq!(
            state.attempt(
                &mut io,
                &mut bus,
                request,
                Some(&mut output),
                1_000_000_000,
                1_000_100_000
            ),
            Ok(1)
        );
        assert_eq!(output, [0x31]);
        assert!(io.steps.is_empty());
        assert!(io.logs.is_empty());
        assert_eq!(state.timing.failure_phase, FailurePhase::None);
    }
    #[test]
    fn internal_response_overhead_is_checked_even_without_caller_output() {
        let mut io = script(2045, &[]);
        io.steps.pop_back();
        io.steps[0].0[4] = 0;
        io.steps[0].0[6] ^= 1;
        io.steps[2].0[5] = 0;
        io.steps[2].0[7] ^= 1;
        for _ in 0..3 {
            let mut reply = vec![0; 1024];
            reply[0] = 0x1f;
            io.steps.push_back((vec![0x14; 1024], reply, 1024));
        }
        let mut state = Protocol::default();
        let mut bus = BusTiming::default();
        let data = [0xa1, 0, 0, 0, 0, 0, 0];
        let request = Request {
            endpoint: 0,
            data: Some(&data),
            maximum: 0,
            timeout_ms: 100,
        };
        assert_eq!(
            state.attempt(
                &mut io,
                &mut bus,
                request,
                None,
                1_000_000_000,
                1_000_100_000
            ),
            Ok(-1)
        );
        assert_eq!(state.timing.failure_phase, FailurePhase::RxLength);
        assert!(io.steps.is_empty());
    }
    #[test]
    fn oversized_reply_recovers_without_receiving_or_copying_payload() {
        for length in [2, 2045, 2047, 2048, u16::MAX] {
            let mut io = script(length, &[]);
            io.steps.pop_back();
            for _ in 0..3 {
                let mut reply = vec![0; 1024];
                reply[0] = 0x1f;
                io.steps.push_back((vec![0x14; 1024], reply, 1024));
            }
            let mut state = Protocol::default();
            let mut bus = BusTiming::default();
            let mut output = [0xa5];
            let data = [0xa1, 0, 0, 0, 0, 1, 0];
            let request = Request {
                endpoint: 0,
                data: Some(&data),
                maximum: 1,
                timeout_ms: 100,
            };
            assert_eq!(
                state.attempt(
                    &mut io,
                    &mut bus,
                    request,
                    Some(&mut output),
                    1_000_000_000,
                    1_000_100_000
                ),
                Ok(-1)
            );
            assert_eq!(output, [0xa5]);
            assert!(io.steps.is_empty());
            assert_eq!(state.timing.failure_phase, FailurePhase::RxLength);
        }
    }
}
