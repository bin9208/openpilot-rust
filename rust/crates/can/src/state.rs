use crate::{dbc::Message, Diagnostic, Error};
use num_bigint::BigInt;
use num_traits::{One, ToPrimitive};
use serde::Serialize;
use std::collections::VecDeque;

#[derive(Debug, Serialize)]
pub struct MessageState {
    pub address: u32,
    pub name: String,
    pub ignore_alive: bool,
    pub ignore_checksum: bool,
    pub ignore_counter: bool,
    pub frequency: f64,
    pub timeout_threshold: f64,
    pub values: Vec<f64>,
    pub all_values: Vec<Vec<f64>>,
    pub timestamps: VecDeque<u64>,
    pub counter: BigInt,
    pub counter_fail: u8,
    pub first_seen: u64,
    pub last_warning: u64,
}

impl MessageState {
    pub fn new(message: &Message, frequency: Option<f64>, now: u64) -> Self {
        let mut state = Self {
            address: message.address,
            name: message.name.clone(),
            ignore_alive: frequency.is_some_and(f64::is_nan),
            ignore_checksum: false,
            ignore_counter: false,
            frequency: frequency.filter(|f| *f > 0.).unwrap_or(0.),
            timeout_threshold: (1e9 / frequency.filter(|f| *f > 0.).unwrap_or(1.)) * 10.,
            values: Vec::new(),
            all_values: Vec::new(),
            timestamps: VecDeque::with_capacity(500),
            counter: BigInt::from(0),
            counter_fail: 0,
            first_seen: now,
            last_warning: 0,
        };
        state.values.resize(message.signals.len(), 0.);
        state
            .all_values
            .resize_with(message.signals.len(), Vec::new);
        state
    }

    pub fn parse(
        &mut self,
        message: &Message,
        packet: (u64, &[u8]),
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Result<bool, Error> {
        let (nanos, data) = packet;
        if self.first_seen == 0 {
            self.first_seen = nanos;
        }
        let mut values = Vec::with_capacity(message.signals.len());
        let mut checksum_failed = false;
        let mut counter_failed = false;
        for signal in &message.signals {
            let raw = signal.raw(data)?;
            // Source checks checksum/counter against the signed raw value, not physical value.
            let mut signed = raw.clone();
            if signal.signed {
                let sign = (&signed >> (signal.size - 1)) & BigInt::one();
                signed -= sign << signal.size;
            }
            if !self.ignore_checksum && signal.kind.is_checksum() {
                let expected = signal
                    .kind
                    .compute(self.address, signal, &mut data.to_vec())?;
                if signed != BigInt::from(expected) {
                    checksum_failed = true;
                    self.log(
                        nanos,
                        format!("checksum failed: received {signed:#x}, calculated {expected:#x}"),
                        diagnostics,
                    );
                }
            }
            if !self.ignore_counter
                && signal.kind == crate::checksum::Kind::Counter
                && !self.update_counter(&signed, signal.size)
            {
                counter_failed = true;
            }
            values.push(signal.physical(&raw)?);
        }
        if checksum_failed || counter_failed {
            return Ok(false);
        }
        self.values = values;
        for (all, value) in self.all_values.iter_mut().zip(&self.values) {
            all.push(*value);
        }
        if self.timestamps.len() == 500 {
            self.timestamps.pop_front();
        }
        self.timestamps.push_back(nanos);
        if self.frequency < 1e-5 && self.timestamps.len() >= 3 {
            let first = self.timestamps.front().copied().ok_or(Error::Numeric)?;
            let dt = (i128::from(nanos) - i128::from(first))
                .to_f64()
                .ok_or(Error::Numeric)?
                * 1e-9;
            if (dt > 1. || self.timestamps.len() >= 500) && dt != 0. {
                self.frequency =
                    (f64::from(u32::try_from(self.timestamps.len()).map_err(|_| Error::Numeric)?)
                        / dt)
                        .min(100.);
                self.timeout_threshold = (1e9 / self.frequency) * 10.;
            }
        }
        Ok(true)
    }

    pub fn update_counter(&mut self, count: &BigInt, size: usize) -> bool {
        if (&self.counter + 1) & ((BigInt::one() << size) - 1) != *count {
            self.counter_fail = (self.counter_fail + 1).min(5);
        } else if self.counter_fail > 0 {
            self.counter_fail -= 1;
        }
        self.counter = count.clone();
        self.counter_fail < 5
    }

    pub fn valid(&self, nanos: u64) -> bool {
        if self.ignore_alive {
            return true;
        }
        let Some(last) = self.timestamps.back() else {
            return self.first_seen != 0
                && i128::from(nanos) - i128::from(self.first_seen) < 2_000_000_000;
        };
        let age = i128::from(nanos) - i128::from(*last);
        age.to_f64()
            .is_some_and(|age| age <= self.timeout_threshold)
    }

    pub(crate) fn log(&mut self, nanos: u64, reason: String, diagnostics: &mut Vec<Diagnostic>) {
        if i128::from(nanos) - i128::from(self.last_warning) >= 1_000_000_000 {
            diagnostics.push(Diagnostic {
                address: self.address,
                message: format!("CANParser: {:#x} {} {reason}", self.address, self.name),
            });
            self.last_warning = nanos;
        }
    }
}
