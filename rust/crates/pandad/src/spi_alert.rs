use serde::Serialize;
use std::collections::VecDeque;

pub const ONROAD_ARM_DELAY_MS: u64 = 5000;
pub const RECOVERED_BURST_WINDOW_MS: u64 = 10000;
pub const RECOVERED_BURST_THRESHOLD: u64 = 3;
pub const CONFIRM_DELAY_MS: u64 = 1000;

#[derive(Default, Serialize)]
pub struct Tracker {
    onroad: bool,
    since: u64,
    pending: bool,
    pending_since: u64,
    captured: bool,
    times: VecDeque<u64>,
}

impl Tracker {
    pub fn update_onroad(&mut self, onroad: bool, now: u64) {
        if !onroad {
            *self = Self::default();
            return;
        }
        if !self.onroad {
            *self = Self {
                onroad: true,
                since: now,
                ..Self::default()
            };
        }
    }

    pub fn observe(&mut self, now: u64, count: u64, terminal: bool) -> bool {
        if !self.armed(now) || self.captured || count == 0 {
            return false;
        }
        while self
            .times
            .front()
            .is_some_and(|time| now.wrapping_sub(*time) > RECOVERED_BURST_WINDOW_MS)
        {
            self.times.pop_front();
        }
        if !terminal {
            self.times.extend(std::iter::repeat_n(
                now,
                count.min(RECOVERED_BURST_THRESHOLD) as usize,
            ));
            if self.times.len() < RECOVERED_BURST_THRESHOLD as usize {
                return false;
            }
        }
        if !self.pending {
            self.pending = true;
            self.pending_since = now;
        }
        true
    }

    pub fn ready(&self, now: u64) -> bool {
        self.armed(now)
            && self.pending
            && !self.captured
            && now.wrapping_sub(self.pending_since) >= CONFIRM_DELAY_MS
    }

    pub fn mark_capture_requested(&mut self) {
        self.captured = true;
        self.pending = false;
        self.times.clear();
    }

    fn armed(&self, now: u64) -> bool {
        self.onroad && now.wrapping_sub(self.since) >= ONROAD_ARM_DELAY_MS
    }
}
