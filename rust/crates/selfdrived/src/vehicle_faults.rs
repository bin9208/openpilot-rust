//! Port of selfdrived/vehicle_faults.py: escalate current controller errors.
//! Historical counters alone are not faults.
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const CAN_CONFIRM_S: f64 = 0.5;
const CAN_ERROR_RATE: f64 = 100.0;
const CAN_SAMPLE_GAP_S: f64 = 0.5;
const RECOVERY_S: f64 = 2.0;
const SCC_WINDOW_S: f64 = 1.0;
const SCC_CONFIRM_S: f64 = 0.10;

/// One CAN controller as reported by `pandaStates`.
#[derive(Clone, Copy, Debug, Default)]
pub struct CanSample {
    pub error_passive: bool,
    pub bus_off: bool,
    pub total_error_count: u32,
    pub interrupt_rate_fault: bool,
}

#[derive(Clone, Debug, Default)]
pub struct PandaSample {
    pub uptime: u32,
    pub buses: [CanSample; 3],
}

#[derive(Debug, Default)]
pub struct VehicleFaultMonitor {
    previous: BTreeMap<(usize, usize), (f64, u32, u32)>,
    can_since: BTreeMap<(usize, usize), f64>,
    can_good_since: Option<f64>,
    can_fault: bool,
    scc_samples: VecDeque<(f64, f64)>,
    scc_last_time: Option<f64>,
    scc_good_since: Option<f64>,
    scc_fault: bool,
}

impl VehicleFaultMonitor {
    pub fn update_pandas(&mut self, pandas: &[PandaSample], time: f64, valid: bool) -> bool {
        if !valid || !time.is_finite() {
            self.can_good_since = None;
            self.can_since.clear();
            return self.can_fault;
        }
        let (mut growing, mut confirmed, mut fresh) = (false, false, false);
        let mut healthy = !pandas.is_empty();
        let mut keys = BTreeSet::new();
        for (index, panda) in pandas.iter().enumerate() {
            for (bus, state) in panda.buses.iter().enumerate() {
                let key = (index, bus);
                keys.insert(key);
                healthy &= !(state.error_passive || state.bus_off);
                let count = state.total_error_count;
                let previous = self.previous.get(&key).copied();
                if previous.is_some_and(|sample| time <= sample.0) {
                    continue;
                }
                fresh = true;
                self.previous.insert(key, (time, count, panda.uptime));
                let dt = previous.map_or(0.0, |sample| time - sample.0);
                let continuous = previous.is_some_and(|sample| {
                    dt <= CAN_SAMPLE_GAP_S && panda.uptime >= sample.2 && count >= sample.1
                });
                if !continuous {
                    self.can_good_since = None;
                }
                let current = previous
                    .filter(|_| continuous && dt > 0.0)
                    .is_some_and(|sample| {
                        f64::from(count - sample.1) / dt >= CAN_ERROR_RATE
                            && (state.error_passive || state.bus_off)
                            && state.interrupt_rate_fault
                    });
                if current {
                    growing = true;
                    let since = *self
                        .can_since
                        .entry(key)
                        .or_insert_with(|| previous.map_or(time, |sample| sample.0));
                    confirmed |= time - since >= CAN_CONFIRM_S;
                } else {
                    self.can_since.remove(&key);
                }
            }
        }
        self.previous.retain(|key, _| keys.contains(key));
        self.can_since.retain(|key, _| keys.contains(key));
        if fresh {
            if growing || !healthy {
                self.can_good_since = None;
            } else if self.can_good_since.is_none() {
                self.can_good_since = Some(time);
            }
            if confirmed {
                self.can_fault = true;
            } else if self.can_fault
                && self
                    .can_good_since
                    .is_some_and(|since| time - since >= RECOVERY_S)
            {
                self.can_fault = false;
            }
        }
        self.can_fault
    }

    pub fn update_scc(&mut self, fault: bool, time: f64, valid: bool) -> bool {
        if !valid || !time.is_finite() {
            self.scc_good_since = None;
            self.scc_samples.clear();
            return self.scc_fault;
        }
        let previous = self.scc_last_time;
        if previous.is_some_and(|last| time <= last) {
            return self.scc_fault;
        }
        self.scc_last_time = Some(time);
        let mut dt = previous.map_or(0.0, |last| time - last);
        if dt > CAN_SAMPLE_GAP_S {
            self.scc_samples.clear();
            self.scc_good_since = None;
            dt = 0.0;
        }
        if fault {
            self.scc_good_since = None;
        } else if self.scc_good_since.is_none() {
            self.scc_good_since = Some(time);
        }
        self.scc_samples
            .push_back((time, if fault { dt.min(0.05) } else { 0.0 }));
        while self
            .scc_samples
            .front()
            .is_some_and(|sample| time - sample.0 > SCC_WINDOW_S)
        {
            self.scc_samples.pop_front();
        }
        if self.scc_samples.iter().map(|sample| sample.1).sum::<f64>() >= SCC_CONFIRM_S - 1e-9 {
            self.scc_fault = true;
        } else if self.scc_fault
            && self
                .scc_good_since
                .is_some_and(|since| time - since >= RECOVERY_S)
        {
            self.scc_fault = false;
        }
        self.scc_fault
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panda(count: u32, bad: bool) -> PandaSample {
        let bus = CanSample {
            error_passive: bad,
            bus_off: false,
            total_error_count: count,
            interrupt_rate_fault: bad,
        };
        PandaSample {
            uptime: 100,
            buses: [bus, CanSample::default(), CanSample::default()],
        }
    }

    #[test]
    fn growing_can_errors_latch_after_confirmation_and_recover_when_healthy() {
        let mut monitor = VehicleFaultMonitor::default();
        let mut count = 0;
        let mut time = 0.0;
        assert!(!monitor.update_pandas(&[panda(count, true)], time, true));
        for _ in 0..4 {
            time += 0.1;
            count += 50;
            assert!(!monitor.update_pandas(&[panda(count, true)], time, true));
        }
        time += 0.1;
        count += 50;
        assert!(monitor.update_pandas(&[panda(count, true)], time, true));
        time += 0.1;
        assert!(monitor.update_pandas(&[panda(count, false)], time, true));
        time += RECOVERY_S + 0.1;
        // A sample gap restarts the recovery window instead of clearing the fault.
        assert!(monitor.update_pandas(&[panda(count, false)], time, true));
        for _ in 0..25 {
            time += 0.1;
            monitor.update_pandas(&[panda(count, false)], time, true);
        }
        assert!(!monitor.update_pandas(&[panda(count, false)], time + 0.1, true));
    }

    #[test]
    fn old_error_counter_without_growth_is_not_a_fault() {
        let mut monitor = VehicleFaultMonitor::default();
        for step in 0..30 {
            assert!(!monitor.update_pandas(&[panda(5000, true)], f64::from(step) * 0.1, true));
        }
    }

    #[test]
    fn scc_fault_needs_confirmed_duration_and_clears_after_recovery() {
        let mut monitor = VehicleFaultMonitor::default();
        let mut time = 0.0;
        assert!(!monitor.update_scc(true, time, true));
        time += 0.05;
        assert!(!monitor.update_scc(true, time, true));
        time += 0.05;
        assert!(monitor.update_scc(true, time, true));
        for _ in 0..50 {
            time += 0.05;
            monitor.update_scc(false, time, true);
        }
        assert!(!monitor.update_scc(false, time + 0.05, true));
    }
}
