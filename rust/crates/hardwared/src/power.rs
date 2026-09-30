use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct PowerMonitoring {
    pub last_measurement: Option<f64>,
    pub last_save: f64,
    pub used: f64,
    pub capacity: f64,
    pub voltage: f64,
    pub instant_voltage: f64,
}
#[derive(Debug, Deserialize)]
pub struct Shutdown {
    pub now: f64,
    pub ignition: bool,
    pub in_car: bool,
    pub off_ts: Option<f64>,
    pub started_seen: bool,
    pub max_offroad_minutes: i64,
    pub disable: bool,
    pub force: bool,
}
impl PowerMonitoring {
    pub fn new(capacity: f64) -> Self {
        Self {
            last_measurement: None,
            last_save: 0.,
            used: 0.,
            capacity: capacity.max(3e6),
            voltage: 12e3,
            instant_voltage: 12e3,
        }
    }
    /// Returns the pre-integration capacity to enqueue in Params, every ten seconds.
    pub fn calculate(
        &mut self,
        now: f64,
        voltage: Option<f64>,
        ignition: bool,
        power: f64,
    ) -> (Option<i64>, Option<String>) {
        self.calculate_with(now, voltage, ignition, || Ok(power))
    }
    pub fn calculate_with(
        &mut self,
        now: f64,
        voltage: Option<f64>,
        ignition: bool,
        power: impl FnOnce() -> Result<f64, String>,
    ) -> (Option<i64>, Option<String>) {
        let Some(voltage) = voltage else {
            self.last_measurement = None;
            self.used = 0.;
            return (None, None);
        };
        self.instant_voltage = voltage;
        self.voltage = voltage * 0.011 + self.voltage * (1. - 0.011);
        self.capacity = self.capacity.clamp(0., 30e6);
        let save = if now - self.last_save >= 10. {
            self.last_save = now;
            Some(self.capacity as i64)
        } else {
            None
        };
        let Some(last) = self.last_measurement else {
            self.last_measurement = Some(now);
            return (save, None);
        };
        let elapsed = (now - last) / 3600.;
        if ignition {
            self.used = 0.;
            if elapsed < 0. {
                return (save, Some("Power monitoring calculation failed".into()));
            }
            self.capacity += 45. * 1e6 * elapsed;
            self.last_measurement = Some(now);
        } else {
            let power = match power() {
                Ok(power) => power,
                Err(error) => return (save, Some(error)),
            };
            if last == 0. {
                return (save, None);
            }
            let used = power * 1e6 * elapsed;
            if used < 0. {
                return (save, Some("Integration failed".into()));
            }
            self.used += used;
            self.capacity -= used;
            self.last_measurement = Some(now);
        }
        (save, None)
    }
    pub fn should_shutdown(&self, input: &Shutdown) -> bool {
        let Some(off_ts) = input.off_ts else {
            return false;
        };
        let elapsed = input.now - off_ts;
        let low = self.voltage < 11800. && elapsed > 60.;
        let shutdown =
            (elapsed > input.max_offroad_minutes as f64 * 60. || low || self.capacity <= 0.)
                && !input.ignition
                && !input.disable
                && input.in_car
                && elapsed > 300.;
        (shutdown || input.force) && (input.started_seen || input.now > 3600.)
    }
}
