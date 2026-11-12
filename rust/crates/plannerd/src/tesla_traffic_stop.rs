//! Tesla-only supplementary stop-line constraint, independent of model and
//! lead stops. Port of carrot's `controls/lib/tesla_traffic_stop.py`.

pub const MAX_AGE_NS: u64 = 500_000_000;
pub const RED_CONFIRM_NS: u64 = 200_000_000;
pub const GREEN_CONFIRM_NS: u64 = 500_000_000;
pub const STOP_BUFFER: f64 = 2.0;
pub const ENTRY_DISTANCE: f64 = 126.0;
/// `TeslaFlags.CAN_TRAFFIC_STOP`.
pub const CAN_TRAFFIC_STOP: u32 = 64;

/// `carState` fields the constraint reads, with `teslaTrafficControl`.
#[derive(Clone, Debug, Default)]
pub struct Car {
    pub v_ego: f64,
    pub gas_pressed: bool,
    pub brake_pressed: bool,
    pub drive: bool,
    pub can_valid: bool,
    pub sensors_invalid: bool,
    pub door_open: bool,
    pub seatbelt_unlatched: bool,
    pub parking_brake: bool,
    pub acc_faulted: bool,
    pub esp_disabled: bool,
    pub valid: bool,
    pub sample: u64,
    pub stop_line_distance: f64,
    /// 1 red, 2 green, 3 yellow, 0 unknown.
    pub light: u8,
    pub status: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Decision {
    pub active: bool,
    pub distance: f64,
    pub hold: bool,
    pub warn: bool,
    pub fault: bool,
    pub reason: String,
    pub required_decel: f64,
}

fn idle(reason: &str) -> Decision {
    Decision {
        reason: reason.into(),
        ..Decision::default()
    }
}

#[derive(Debug, Default)]
pub struct TeslaTrafficStop {
    active: bool,
    distance: f64,
    last_now: u64,
    last_sample: u64,
    supported_sample: u64,
    pending_color: u8,
    pending_since: u64,
    pending_count: u32,
    pending_distance: f64,
    pending_now: u64,
    pending_sample: u64,
    fault: String,
    overridden: bool,
}

impl TeslaTrafficStop {
    fn clear(&mut self, now: u64) {
        self.active = false;
        self.pending_color = 0;
        self.pending_since = 0;
        self.pending_count = 0;
        self.supported_sample = 0;
        self.fault.clear();
        self.last_sample = self.last_sample.max(now);
    }

    fn reset_pending(&mut self) {
        self.pending_color = 0;
        self.pending_since = 0;
        self.pending_count = 0;
    }

    pub fn update(&mut self, cs: &Car, now: u64, engaged: bool, control_active: bool) -> Decision {
        let dt = if self.last_now != 0 {
            (now as f64 - self.last_now as f64) / 1e9
        } else {
            0.
        };
        self.last_now = now;
        if !engaged {
            self.overridden = false;
        }
        if cs.gas_pressed || cs.brake_pressed {
            self.overridden = true;
        }
        let speed = cs.v_ego;
        if !control_active
            || !engaged
            || self.overridden
            || !speed.is_finite()
            || speed < 0.
            || !cs.drive
            || !cs.can_valid
            || cs.sensors_invalid
            || cs.door_open
            || cs.seatbelt_unlatched
            || cs.parking_brake
            || cs.acc_faulted
            || cs.esp_disabled
        {
            self.clear(now);
            return idle(if self.overridden {
                "driver_override"
            } else {
                "inactive"
            });
        }
        if self.active {
            if !(dt > 0. && dt <= 0.2) {
                self.fault = "clock_discontinuity".into();
            }
            self.distance -= speed * dt.clamp(0., 0.2);
            if self.distance < -5. {
                self.active = false;
                self.fault = "passed_stop_line".into();
            }
        }
        let sample = cs.sample;
        let mut valid = cs.valid
            && 0 < sample
            && sample <= now
            && now - sample <= MAX_AGE_NS
            && cs.stop_line_distance.is_finite()
            && (0. ..255.).contains(&cs.stop_line_distance)
            && matches!(cs.light, 1..=3);
        if sample < self.last_sample {
            valid = false;
        }
        if sample > self.last_sample {
            self.last_sample = sample;
            if valid && self.fault.is_empty() {
                let distance = cs.stop_line_distance - speed * ((now - sample) as f64 / 1e9);
                let color = cs.light;
                let tolerance = (speed * 0.15).max(3.);
                if self.active && (distance - self.distance).abs() > tolerance {
                    self.fault = "stop_line_jump".into();
                } else if matches!(color, 1 | 2) {
                    self.supported_sample = sample;
                    let consistent = self.pending_color == color
                        && self.pending_since > 0
                        && sample > self.pending_sample
                        && sample - self.pending_sample <= MAX_AGE_NS
                        && (distance
                            - (self.pending_distance
                                - speed * (now as f64 - self.pending_now as f64) / 1e9))
                            .abs()
                            <= tolerance;
                    if !consistent {
                        self.pending_since = sample;
                        self.pending_count = 0;
                    }
                    self.pending_color = color;
                    self.pending_count += 1;
                    self.pending_distance = distance;
                    self.pending_now = now;
                    self.pending_sample = sample;
                    let elapsed = sample - self.pending_since;
                    if self.active {
                        // Never move an owned stop line away on a range spike.
                        self.distance = self.distance.min(distance);
                        if color == 2 && self.pending_count >= 3 && elapsed >= GREEN_CONFIRM_NS {
                            self.clear(now);
                            return idle("green_released");
                        }
                    } else if color == 1
                        && 0. < distance
                        && distance < ENTRY_DISTANCE
                        && self.pending_count >= 3
                        && elapsed >= RED_CONFIRM_NS
                    {
                        self.active = true;
                        self.distance = distance;
                    }
                } else {
                    self.reset_pending();
                }
            } else {
                self.reset_pending();
            }
        } else if !valid {
            self.reset_pending();
        }
        if self.active
            && now.saturating_sub(self.supported_sample) > MAX_AGE_NS
            && self.fault.is_empty()
        {
            self.fault = "traffic_signal_lost".into();
        }
        if !self.active {
            if !valid {
                self.reset_pending();
            }
            let fault = !self.fault.is_empty();
            let reason = if fault {
                self.fault.clone()
            } else if !cs.status.is_empty() {
                cs.status.clone()
            } else {
                "confirming".into()
            };
            return Decision {
                warn: fault,
                fault,
                reason,
                ..Decision::default()
            };
        }
        let available = self.distance - STOP_BUFFER - speed * 0.3;
        let required = speed * speed / (2. * available.max(0.1));
        let hold = speed < 0.3 && self.distance <= STOP_BUFFER + 1.;
        // A feasibility warning, never permission to raise actuator limits.
        let late = required > 3.48 && !hold;
        let fault = !self.fault.is_empty();
        let reason = if fault {
            self.fault.clone()
        } else if late {
            "insufficient_distance".into()
        } else if hold {
            "holding".into()
        } else {
            "stopping".into()
        };
        Decision {
            active: true,
            distance: self.distance.max(0.),
            hold,
            warn: fault || late,
            fault,
            reason,
            required_decel: required,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn car(sample: u64, distance: f64, light: u8, speed: f64) -> Car {
        Car {
            v_ego: speed,
            drive: true,
            can_valid: true,
            valid: true,
            sample,
            stop_line_distance: distance,
            light,
            ..Car::default()
        }
    }

    #[test]
    fn red_confirms_then_green_releases() {
        let mut stop = TeslaTrafficStop::default();
        let mut now = 1_000_000_000u64;
        let mut distance = 80.;
        let mut decision = Decision::default();
        for _ in 0..6 {
            now += 100_000_000;
            distance -= 1.;
            decision = stop.update(&car(now, distance, 1, 10.), now, true, true);
        }
        assert!(decision.active, "{decision:?}");
        assert!(!decision.hold);
        let mut released = false;
        for _ in 0..8 {
            now += 100_000_000;
            distance -= 1.;
            decision = stop.update(&car(now, distance, 2, 10.), now, true, true);
            released |= decision.reason == "green_released";
        }
        assert!(!decision.active);
        assert!(released);
    }

    #[test]
    fn driver_input_overrides_until_disengaged() {
        let mut stop = TeslaTrafficStop::default();
        let mut cs = car(1, 50., 1, 10.);
        cs.gas_pressed = true;
        assert_eq!(stop.update(&cs, 2, true, true).reason, "driver_override");
        cs.gas_pressed = false;
        assert_eq!(stop.update(&cs, 3, true, true).reason, "driver_override");
        assert_eq!(stop.update(&cs, 4, false, true).reason, "inactive");
    }
}
