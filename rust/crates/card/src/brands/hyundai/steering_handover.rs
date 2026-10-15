//! Source angle-control authority handover; angle commands remain independently limited.
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Waiting,
    Blocked,
    Capture,
    Recover,
    Offering,
    Withdrawing,
    Returning,
}
impl State {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Blocked => "blocked",
            Self::Capture => "capture",
            Self::Recover => "recover",
            Self::Offering => "offering",
            Self::Withdrawing => "withdrawing",
            Self::Returning => "returning",
        }
    }
}
#[derive(Serialize)]
pub struct SteeringHandover {
    pub mode: i32,
    pub last_time: Option<f64>,
    pub raw_history: VecDeque<f64>,
    pub history: VecDeque<(f64, f64, f64)>,
    pub force_history: VecDeque<(f64, f64)>,
    pub effort: Option<f64>,
    pub error: f64,
    pub state: State,
    pub cap: f64,
    pub qualify: f64,
    pub release: f64,
    pub strong: f64,
    pub armed_at: Option<f64>,
    pub release_qualify: f64,
    pub offer_since: f64,
    pub offer_effort: f64,
    pub offer_sign: f64,
    pub capture_effort: f64,
    pub capture_since: f64,
    pub previous_driver: f64,
    pub direction_time: f64,
}
impl Default for SteeringHandover {
    fn default() -> Self {
        Self {
            mode: 0,
            last_time: None,
            raw_history: VecDeque::new(),
            history: VecDeque::new(),
            force_history: VecDeque::new(),
            effort: None,
            error: 0.,
            state: State::Waiting,
            cap: 0.,
            qualify: 0.,
            release: 0.,
            strong: 0.,
            armed_at: None,
            release_qualify: 0.,
            offer_since: 0.,
            offer_effort: 0.,
            offer_sign: 0.,
            capture_effort: 0.,
            capture_since: 0.,
            previous_driver: 0.,
            direction_time: 0.,
        }
    }
}
#[derive(Clone, Copy, Deserialize)]
pub struct Input {
    pub mode: i32,
    pub now: f64,
    pub baseline: f64,
    pub minimum: f64,
    pub maximum: f64,
    pub driver: f64,
    pub threshold: f64,
    pub pressed: bool,
    pub target_error: f64,
    pub command_error: f64,
    pub speed: f64,
    pub wheelbase: f64,
    pub steer_ratio: f64,
    pub active: bool,
    pub valid: bool,
}
fn approach(value: f64, target: f64, step: f64) -> f64 {
    value + (-step).max(step.min(target - value))
}
impl SteeringHandover {
    pub fn reset(&mut self, mode: i32, now: Option<f64>) {
        *self = Self {
            mode,
            last_time: now,
            ..Self::default()
        };
    }
    pub fn update(&mut self, i: Input) -> f64 {
        use State::*;
        let mode = if (1..=3).contains(&i.mode) { i.mode } else { 0 };
        if mode == 0 || mode != self.mode || !i.active {
            self.reset(mode, Some(i.now));
            return i.baseline;
        }
        let dt = self.last_time.map_or(0., |t| i.now - t);
        self.last_time = Some(i.now);
        let valid = i.valid
            && [
                i.now,
                dt,
                i.baseline,
                i.minimum,
                i.maximum,
                i.driver,
                i.threshold,
                i.target_error,
                i.command_error,
                i.speed,
                i.wheelbase,
                i.steer_ratio,
            ]
            .iter()
            .all(|v| v.is_finite())
            && dt > 0.
            && dt <= 0.03
            && i.threshold > 0.
            && i.wheelbase > 0.
            && i.steer_ratio > 0.
            && 0. <= i.minimum
            && i.minimum < i.maximum
            && 0. <= i.baseline
            && i.baseline <= i.maximum;
        if !valid {
            let owned = self.state != Waiting;
            let cap = self.cap;
            self.reset(mode, Some(i.now));
            if owned {
                self.state = Blocked;
                self.cap = i.baseline.min(cap);
                return self.cap;
            }
            return i.baseline;
        }
        let raw = i.driver.abs() / i.threshold;
        self.direction_time = if i.driver * self.previous_driver > 0. {
            self.direction_time + dt
        } else {
            0.
        };
        self.previous_driver = i.driver;
        if self.raw_history.len() == 3 {
            self.raw_history.pop_front();
        }
        self.raw_history.push_back(raw);
        let mut values = self.raw_history.iter().copied().collect::<Vec<_>>();
        values.sort_by(f64::total_cmp);
        let magnitude = if values.len() % 2 == 1 {
            values[values.len() / 2]
        } else {
            (values[values.len() / 2 - 1] + values[values.len() / 2]) / 2.
        };
        self.effort = Some(
            self.effort
                .map_or(magnitude, |e| e + dt / (0.12 + dt) * (magnitude - e)),
        );
        let effort = self.effort.expect("effort initialized");
        let error = i.target_error.abs().max(i.command_error.abs());
        self.error += dt / (0.08 + dt) * (error - self.error);
        self.history.push_back((i.now, effort, self.error));
        while self.history.len() > 1 && i.now - self.history[0].0 > 0.22 {
            self.history.pop_front();
        }
        let span = i.now - self.history[0].0;
        let effort_change = effort - self.history[0].1;
        let error_change = self.error - self.history[0].2;
        let margin = 3_f64.min(
            (0.8 * i.wheelbase / (i.speed * i.speed).max(25.))
                .atan()
                .to_degrees()
                * i.steer_ratio,
        );
        let error_quality = 0_f64.max(1_f64.min((2. * margin - error.max(self.error)) / margin));
        let converging = error.max(self.error) <= margin || error_change < -0.05;
        let yielding = effort_change <= 0.05;
        let low = !i.pressed && raw <= 0.6 && magnitude <= 0.6;
        self.release = if low { self.release + dt } else { 0. };
        self.strong = if i.pressed && raw > 1. && magnitude > 1. && self.direction_time > 0. {
            self.strong + dt
        } else {
            0.
        };
        if self.strong >= 0.15 {
            self.armed_at = Some(i.now);
        }
        self.force_history.push_back((i.now, raw));
        while self.force_history.len() > 1 && i.now - self.force_history[0].0 > 0.4 {
            self.force_history.pop_front();
        }
        let force_span = i.now - self.force_history[0].0;
        let fall = (self.force_history[0].1 - magnitude) / force_span.max(dt);
        let peak = self
            .force_history
            .iter()
            .map(|v| v.1)
            .fold(f64::NEG_INFINITY, f64::max);
        let candidate = self.armed_at.is_some_and(|t| i.now - t <= 0.5)
            && !i.pressed
            && raw <= 1.
            && magnitude <= 1.
            && peak - magnitude >= 0.25
            && peak >= 1.05
            && fall >= 0.5;
        self.release_qualify = if candidate {
            self.release_qualify + dt
        } else {
            0.
        };
        if (mode == 2 || mode == 3)
            && self.release_qualify >= 0.03
            && i.baseline <= 80.
            && !matches!(self.state, Capture | Recover | Returning)
        {
            if self.state == Waiting {
                self.cap = i.baseline;
            }
            self.state = Capture;
            self.capture_since = i.now;
            self.capture_effort = raw;
            self.armed_at = None;
            self.release_qualify = 0.;
        }
        if matches!(self.state, Capture | Recover) {
            let renewed = i.pressed
                || raw
                    > if self.state == Capture {
                        0.8_f64.max(self.capture_effort + 0.15)
                    } else {
                        0.8
                    };
            if renewed {
                self.state = Blocked;
            } else if self.state == Capture {
                if self.release >= 0.06 {
                    self.state = Recover;
                } else if i.now - self.capture_since >= 0.3 {
                    self.state = Withdrawing;
                    self.offer_effort = effort;
                    self.offer_sign = i.driver;
                }
            }
        }
        if matches!(self.state, Offering | Withdrawing) {
            let reversed = i.driver * self.offer_sign < 0. && raw > 0.7;
            let renewed = reversed
                || raw > 3.2
                || raw > self.offer_effort + 0.4
                || effort > self.offer_effort + 0.15
                || effort_change > 0.2;
            let elapsed = i.now - self.offer_since;
            if renewed {
                self.state = Blocked;
            } else if self.state == Offering && self.release >= 0.1 {
                self.state = Returning;
            } else if self.state == Offering
                && ((elapsed >= 0.6 && self.offer_effort - effort < 0.1) || elapsed >= 1.5)
            {
                self.state = Withdrawing;
            }
        }
        let eligible = (mode == 1 || mode == 3)
            && self.state == Waiting
            && span >= 0.18
            && i.pressed
            && self.direction_time >= 0.2
            && 0.6 < effort
            && effort < 3.
            && raw <= 3.2_f64.min(effort + 0.4)
            && i.baseline <= 80.
            && error_quality > 0.
            && converging
            && yielding;
        self.qualify = if eligible { self.qualify + dt } else { 0. };
        if self.qualify >= 0.25 {
            self.state = Offering;
            self.offer_since = i.now;
            self.offer_effort = effort;
            self.offer_sign = i.driver;
            self.cap = i.baseline;
            self.qualify = 0.;
        }
        match self.state {
            Offering => {
                let strength = raw.max(effort);
                let force_cap = if strength <= 1.3 {
                    80.
                } else if strength <= 2. {
                    80. - (strength - 1.3) * 50.
                } else {
                    i.minimum.max(45. - (strength - 2.) * 20.)
                };
                let desired = i.minimum + (i.maximum.min(force_cap) - i.minimum) * error_quality;
                if desired <= self.cap || (converging && yielding) {
                    self.cap = approach(self.cap, desired, 110. * dt);
                }
            }
            Capture => self.cap = approach(self.cap, i.maximum.min(45.), 110. * dt),
            Recover => {
                let quality = 0.25_f64.max(1_f64.min(margin / margin.max(error).max(self.error)));
                self.cap = approach(
                    self.cap,
                    i.maximum,
                    (i.maximum - i.minimum) / 0.5 * quality * dt,
                );
                if self.cap == i.maximum && i.baseline == i.maximum {
                    self.state = Waiting;
                }
            }
            Withdrawing => {
                self.cap = approach(self.cap, i.minimum, 110. * dt);
                if self.cap == i.minimum {
                    self.state = Blocked;
                }
            }
            Blocked => {
                self.cap = approach(self.cap, i.minimum, 2000. * dt);
                if self.release >= 0.2 && self.cap == i.minimum {
                    self.state = Returning;
                }
            }
            Returning => {
                if i.pressed || raw > 0.8 {
                    self.state = Blocked;
                    self.cap = approach(self.cap, i.minimum, 2000. * dt);
                } else {
                    self.cap = approach(self.cap, self.cap.max(i.baseline), 110. * dt);
                    if self.cap == i.baseline {
                        self.state = Waiting;
                    }
                }
            }
            Waiting => {}
        }
        if self.state == Waiting {
            self.cap = i.baseline;
            return i.baseline;
        }
        self.cap = i.minimum.max(i.maximum.min(self.cap));
        self.cap
    }
}

/// Source controller freshness clock advances only when model frameId changes.
#[derive(Default)]
pub struct ModelFreshness {
    pub frame: Option<u32>,
    pub time: u64,
}
impl ModelFreshness {
    pub fn update(&mut self, frame: Option<u32>, y_std: Option<f64>, now: u64) -> bool {
        if frame.is_some() && frame != self.frame {
            self.frame = frame;
            self.time = now;
        }
        frame.is_some()
            && now >= self.time
            && now - self.time <= 150_000_000
            && y_std.is_some_and(|value| value.is_finite() && (0. ..=0.3).contains(&value))
    }
}
