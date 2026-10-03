use crate::protocol::{AttemptTiming, FailurePhase, Request};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};

#[derive(Clone, Debug, Default, Serialize)]
pub struct ErrorEvent {
    pub sequence: u64,
    pub endpoint: u8,
    pub attempt: u32,
    pub result: i32,
    pub final_result: i32,
    pub attempts: u32,
    pub recoveries: u32,
    pub tx_len: u16,
    pub max_rx_len: u16,
    pub timeout_ms: u32,
    pub phase: String,
    pub lock_us: u64,
    pub turnaround_us: u64,
    pub hack_us: u64,
    pub dack_us: u64,
    pub recovery_us: u64,
    pub total_us: u64,
    pub recovery_restarts: u32,
}
#[derive(Default)]
pub(crate) struct RetryStats {
    pub attempts: u32,
    nacks: u32,
    hack_nacks: u32,
    dack_nacks: u32,
    ack_timeouts: u32,
    host_checksums: u32,
    other_failures: u32,
    recoveries: u32,
    recovery_restarts: u32,
    lock_max_us: u64,
    turnaround_max_us: u64,
    hack_max_us: u64,
    dack_max_us: u64,
    recovery_max_us: u64,
    first: AttemptTiming,
    last: AttemptTiming,
    first_result: i32,
}
impl RetryStats {
    pub fn observe(&mut self, result: i32, timing: AttemptTiming) {
        self.attempts = self.attempts.wrapping_add(1);
        self.nacks = self.nacks.wrapping_add(u32::from(result == -2));
        self.ack_timeouts = self.ack_timeouts.wrapping_add(u32::from(result == -3));
        self.hack_nacks = self
            .hack_nacks
            .wrapping_add(u32::from(timing.failure_phase == FailurePhase::HackNack));
        self.dack_nacks = self
            .dack_nacks
            .wrapping_add(u32::from(timing.failure_phase == FailurePhase::DackNack));
        self.host_checksums = self
            .host_checksums
            .wrapping_add(u32::from(timing.failure_phase == FailurePhase::RxChecksum));
        if result < 0 {
            if self.recoveries == 0 {
                self.first = timing;
                self.first_result = result;
            }
            self.last = timing;
            self.recoveries = self.recoveries.wrapping_add(1);
        }
        self.recovery_restarts = self
            .recovery_restarts
            .wrapping_add(timing.recovery_restarts);
        let other = result < 0
            && !matches!(
                timing.failure_phase,
                FailurePhase::HackNack
                    | FailurePhase::DackNack
                    | FailurePhase::HackTimeout
                    | FailurePhase::DackTimeout
                    | FailurePhase::RxChecksum
            );
        self.other_failures = self.other_failures.wrapping_add(u32::from(other));
        self.lock_max_us = self.lock_max_us.max(timing.lock_us);
        self.turnaround_max_us = self.turnaround_max_us.max(timing.turnaround_us);
        self.hack_max_us = self.hack_max_us.max(timing.hack_us);
        self.dack_max_us = self.dack_max_us.max(timing.dack_us);
        self.recovery_max_us = self.recovery_max_us.max(timing.recovery_us);
    }
}
#[derive(Default)]
struct PhaseStats {
    count: u32,
    slow: u32,
    retries: u32,
    max_attempts: u32,
    total_sum_us: u64,
    total_max_us: u64,
    totals: RetryStats,
}
#[derive(Default)]
pub(crate) struct Diagnostics {
    phases: Mutex<[PhaseStats; 2]>,
    event: Mutex<ErrorEvent>,
    sequence: AtomicU64,
}
impl Diagnostics {
    pub fn event(&self) -> ErrorEvent {
        self.event.lock().expect("SPI event mutex poisoned").clone()
    }
    pub fn sequence(&self) -> u64 {
        self.sequence.load(Ordering::Acquire)
    }
    pub fn record(
        &self,
        request: Request<'_>,
        result: i32,
        total_us: u64,
        stats: &RetryStats,
    ) -> Vec<String> {
        let mut logs = Vec::new();
        if let Some(index) = match request.endpoint {
            3 => Some(0),
            0x81 => Some(1),
            _ => None,
        } {
            if let Some(message) = self.phase(index, request.endpoint, total_us, stats) {
                logs.push(message);
            }
        }
        if stats.recoveries > 0 {
            let t = stats.first;
            let mut event = self.event.lock().expect("SPI event mutex poisoned");
            *event = ErrorEvent {
                sequence: event.sequence.wrapping_add(1),
                endpoint: request.endpoint,
                attempt: 1,
                result: stats.first_result,
                final_result: result,
                attempts: stats.attempts,
                recoveries: stats.recoveries,
                tx_len: request.data.map_or(0, |data| data.len() as u16),
                max_rx_len: request.maximum,
                timeout_ms: request.timeout_ms,
                phase: t.failure_phase.name().into(),
                lock_us: t.lock_us,
                turnaround_us: t.turnaround_us,
                hack_us: t.hack_us,
                dack_us: t.dack_us,
                recovery_us: t.recovery_us,
                total_us: t.total_us,
                recovery_restarts: t.recovery_restarts,
            };
            self.sequence.store(event.sequence, Ordering::Release);
            drop(event);
            let last = stats.last;
            logs.push(format!(concat!("spi_failure_diag: endpoint=0x{:x}, attempts={}, final_ret={}",
                ", hack_nacks={}, dack_nacks={}, ack_timeouts={}, host_checksums={}",
                ", other_failures={}, first_phase={}, last_phase={}",
                ", first_lock_us={}, first_turnaround_us={}, first_hack_us={}, first_dack_us={}, first_recovery_us={}",
                ", last_lock_us={}, last_turnaround_us={}, last_hack_us={}, last_dack_us={}, last_recovery_us={}, recovery_restarts={}"),
                request.endpoint, stats.attempts, result, stats.hack_nacks, stats.dack_nacks, stats.ack_timeouts, stats.host_checksums,
                stats.other_failures, t.failure_phase.name(), last.failure_phase.name(), t.lock_us, t.turnaround_us, t.hack_us,
                t.dack_us, t.recovery_us, last.lock_us, last.turnaround_us, last.hack_us, last.dack_us, last.recovery_us, stats.recovery_restarts));
        }
        logs
    }
    fn phase(
        &self,
        index: usize,
        endpoint: u8,
        total_us: u64,
        stats: &RetryStats,
    ) -> Option<String> {
        let mut phases = self.phases.lock().expect("SPI phase mutex poisoned");
        let s = &mut phases[index];
        s.count = s.count.wrapping_add(1);
        s.slow = s.slow.wrapping_add(u32::from(total_us > 5000));
        s.retries = s.retries.wrapping_add(u32::from(stats.attempts > 1));
        s.max_attempts = s.max_attempts.max(stats.attempts);
        s.total_sum_us = s.total_sum_us.wrapping_add(total_us);
        s.total_max_us = s.total_max_us.max(total_us);
        let t = &mut s.totals;
        t.nacks = t.nacks.wrapping_add(stats.nacks);
        t.hack_nacks = t.hack_nacks.wrapping_add(stats.hack_nacks);
        t.dack_nacks = t.dack_nacks.wrapping_add(stats.dack_nacks);
        t.ack_timeouts = t.ack_timeouts.wrapping_add(stats.ack_timeouts);
        t.host_checksums = t.host_checksums.wrapping_add(stats.host_checksums);
        t.other_failures = t.other_failures.wrapping_add(stats.other_failures);
        t.recoveries = t.recoveries.wrapping_add(stats.recoveries);
        t.recovery_restarts = t.recovery_restarts.wrapping_add(stats.recovery_restarts);
        t.lock_max_us = t.lock_max_us.max(stats.lock_max_us);
        t.turnaround_max_us = t.turnaround_max_us.max(stats.turnaround_max_us);
        t.hack_max_us = t.hack_max_us.max(stats.hack_max_us);
        t.dack_max_us = t.dack_max_us.max(stats.dack_max_us);
        t.recovery_max_us = t.recovery_max_us.max(stats.recovery_max_us);
        if s.count < 100 {
            return None;
        }
        let s = std::mem::take(s);
        drop(phases);
        let t = s.totals;
        if s.retries == 0
            && [
                t.nacks,
                t.hack_nacks,
                t.dack_nacks,
                t.ack_timeouts,
                t.host_checksums,
                t.other_failures,
                t.recoveries,
                t.recovery_restarts,
            ]
            .iter()
            .all(|n| *n == 0)
        {
            return None;
        }
        Some(format!(concat!("spi_phase_diag: endpoint=0x{:x}, total_avg_us={}, total_max_us={}",
            ", lock_max_us={}, turnaround_max_us={}, hack_max_us={}, dack_max_us={}, recovery_max_us={}",
            ", slow_over_5ms={}, retries={}, nacks={}, ack_timeouts={}, max_attempts={}",
            ", hack_nacks={}, dack_nacks={}, host_checksums={}, other_failures={}, recoveries={}, recovery_restarts={}"),
            endpoint, s.total_sum_us / u64::from(s.count), s.total_max_us, t.lock_max_us, t.turnaround_max_us, t.hack_max_us,
            t.dack_max_us, t.recovery_max_us, s.slow, s.retries, t.nacks, t.ack_timeouts, s.max_attempts,
            t.hack_nacks, t.dack_nacks, t.host_checksums, t.other_failures, t.recoveries, t.recovery_restarts))
    }
}
