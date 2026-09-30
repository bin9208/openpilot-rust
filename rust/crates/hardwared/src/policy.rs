use crate::fan::FanController;
use openpilot_runtime_core::filters::FirstOrderFilter;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Input {
    pub now: f64,
    pub frame: i64,
    pub panda_updated: bool,
    pub panda_present: bool,
    pub panda_receive_time: f64,
    pub ignition: bool,
    pub in_car: bool,
    pub cycle_requested: bool,
    pub offroad_temperature: f64,
    pub pmic_temperature: f64,
    pub startup: BTreeMap<String, bool>,
    pub booted: bool,
    pub tesla: bool,
    pub brightness: f64,
}
#[derive(Debug, Serialize)]
pub struct Output {
    pub started: bool,
    pub started_ts: Option<f64>,
    pub thermal: u16,
    pub max_temperature: f64,
    pub offroad_temperature: f64,
    pub fan: i32,
    pub power_save: bool,
    pub power_save_changed: bool,
    pub temperature_alert: bool,
    pub reset_engaged: bool,
    pub rising_edge: bool,
    pub should_start: bool,
    pub startup_changed: bool,
    pub block_duration: Option<f64>,
}
pub struct Policy {
    pub ignition: bool,
    pub in_car: bool,
    pub started_ts: Option<f64>,
    pub off_ts: Option<f64>,
    pub started_seen: bool,
    pub thermal: u16,
    pub count: u64,
    pub onroad: BTreeMap<String, bool>,
    pub startup: BTreeMap<String, bool>,
    pub startup_previous: BTreeMap<String, bool>,
    blocked_ts: Option<f64>,
    cycle_frame: i64,
    temperature_good: bool,
    booted: bool,
    previous_start: bool,
    power_save: bool,
    all_filter: FirstOrderFilter,
    offroad_filter: FirstOrderFilter,
    fan: FanController,
}
impl Policy {
    pub fn new(device: &str) -> Self {
        Self {
            ignition: false,
            in_car: false,
            started_ts: None,
            off_ts: None,
            started_seen: false,
            thermal: 1,
            count: 0,
            onroad: BTreeMap::new(),
            startup: BTreeMap::new(),
            startup_previous: BTreeMap::new(),
            blocked_ts: None,
            cycle_frame: 0,
            temperature_good: true,
            booted: false,
            previous_start: false,
            power_save: false,
            all_filter: FirstOrderFilter::new(0., 5., 0.5, false),
            offroad_filter: FirstOrderFilter::new(0., 5., 0.5, false),
            fan: FanController::new(2, device),
        }
    }
    /// Panda polling runs at 10 Hz; expensive updates run at 2 Hz and on ignition edges.
    pub fn poll(&mut self, input: &Input) -> (bool, bool) {
        if input.cycle_requested {
            self.cycle_frame = input.frame;
        }
        let mut timed_out = false;
        if input.panda_updated && input.panda_present {
            self.ignition = input.ignition;
            self.in_car = input.in_car;
        } else if input.now - input.panda_receive_time > 5. && self.ignition {
            self.ignition = false;
            timed_out = true;
        }
        self.onroad.insert("ignition".into(), self.ignition);
        self.onroad.insert(
            "not_onroad_cycle".into(),
            input.frame - self.cycle_frame >= 10,
        );
        self.onroad
            .insert("device_temp_good".into(), self.temperature_good);
        let edge = self.started_ts.is_some() != self.onroad.values().all(|v| *v);
        (input.frame % 5 == 0 || edge, timed_out)
    }
    pub fn booted(&self) -> bool {
        self.booted
    }
    pub fn step(&mut self, input: &Input) -> Output {
        let off = self.offroad_filter.update(input.offroad_temperature);
        let all = self
            .all_filter
            .update(input.offroad_temperature.max(input.pmic_temperature));
        let fan = self.fan.update(all, self.ignition);
        let off_five = self.started_ts.is_none()
            && (!self.started_seen || self.off_ts.is_none_or(|t| input.now - t > 300.));
        if off_five && off > 75. {
            self.thermal = 3;
        } else {
            let (low, high) = match self.thermal {
                0 => (None, Some(80.)),
                1 => (Some(75.), Some(96.)),
                2 => (Some(88.), Some(107.)),
                _ => (Some(94.), None),
            };
            if low.is_some_and(|v| all < v) {
                self.thermal -= 1;
            } else if high.is_some_and(|v| all > v) {
                self.thermal += 1;
            }
        }
        self.startup = input.startup.clone();
        self.booted |= input.booted;
        self.startup.insert("device_booted".into(), self.booted);
        self.startup
            .insert("device_temp_engageable".into(), self.thermal < 2);
        self.temperature_good = self.thermal < 3;
        self.onroad
            .insert("device_temp_good".into(), self.temperature_good);
        let mut start = self.onroad.values().all(|v| *v);
        if self.started_ts.is_none() {
            start &= self.startup.values().all(|v| *v);
        }
        let reset_engaged = start != self.previous_start || self.count == 0;
        let power_save = !input.tesla && !self.ignition && input.brightness < 1e-3;
        let power_save_changed = power_save != self.power_save || self.count == 0;
        self.power_save = power_save;
        let mut block_duration = None;
        let mut startup_changed = false;
        if start {
            self.off_ts = None;
            if self.started_ts.is_none() {
                self.started_ts = Some(input.now);
                self.started_seen = true;
                block_duration = self.blocked_ts.map(|ts| input.now - ts);
            }
            self.blocked_ts = None;
        } else {
            if self.ignition && self.startup != self.startup_previous {
                startup_changed = true;
                self.startup_previous = self.startup.clone();
                self.blocked_ts = Some(input.now);
            }
            self.started_ts = None;
            if self.off_ts.is_none() {
                self.off_ts = Some(input.now);
            }
        }
        if input.tesla && self.started_ts.is_none() && self.started_seen {
            self.started_ts = Some(input.now);
        }
        let rising_edge = start && !self.previous_start;
        self.previous_start = start;
        self.count += 1;
        Output {
            started: self.started_ts.is_some(),
            started_ts: self.started_ts,
            thermal: self.thermal,
            max_temperature: all,
            offroad_temperature: off,
            fan,
            power_save,
            power_save_changed,
            temperature_alert: (!self.temperature_good || self.thermal >= 2) && self.ignition,
            reset_engaged,
            rising_edge,
            should_start: start,
            startup_changed,
            block_duration,
        }
    }
}
