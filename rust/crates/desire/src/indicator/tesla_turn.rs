//! Tesla lamp-only turn lifetime; initiation still belongs to the existing
//! turn desire. Port of carrot's `desire_lib/tesla_turn.py`.
use crate::types::Car;
use std::f64::consts::PI;

#[derive(Clone, Debug, Default)]
pub struct TeslaTurnJourney {
    pub direction: u8,
    started: f64,
    peak_angle: f64,
    rotation: f64,
    entered: bool,
    /// `(logMonoTime ns, heading)` of the last pose.
    pose: Option<(u64, f64)>,
    nav: String,
    distance: Option<f64>,
    pending: Option<(u8, f64)>,
    pub consumed: bool,
}

pub struct JourneyInput<'a> {
    pub now: f64,
    pub car: &'a Car,
    pub pose: Option<(u64, f64)>,
    pub nav: &'a str,
    pub distance: Option<f64>,
    pub desire: u8,
    pub permitted: bool,
    pub lateral_active: bool,
}

impl TeslaTurnJourney {
    fn geometry(&mut self, car: &Car, pose: Option<(u64, f64)>) -> bool {
        let previous = std::mem::replace(&mut self.pose, pose);
        let Some(pose) = pose.filter(|p| {
            car.v_ego.is_finite() && car.steering_angle_deg.is_finite() && p.1.is_finite()
        }) else {
            self.pose = None;
            self.peak_angle = 0.;
            self.rotation = 0.;
            self.entered = false;
            return false;
        };
        let Some(previous) = previous else {
            return false;
        };
        if pose.0 == previous.0 {
            return false;
        }
        let delta = (pose.1 - previous.1 + PI).rem_euclid(2. * PI) - PI;
        if !(pose.0 > previous.0 && pose.0 - previous.0 <= 350_000_000) || delta.abs() > 0.2 {
            self.peak_angle = 0.;
            self.rotation = 0.;
            self.entered = false;
            return false;
        }
        if car.v_ego < 1. / 3.6 {
            return false;
        }
        let sign = if self.direction == 1 { 1. } else { -1. };
        self.peak_angle = self.peak_angle.max(car.steering_angle_deg * sign);
        self.rotation = (self.rotation - delta * sign).max(0.);
        self.entered |= self.peak_angle >= 30. && self.rotation >= 20f64.to_radians();
        self.entered && self.peak_angle > 30. && car.steering_angle_deg.abs() <= 15.
    }

    pub fn update(&mut self, i: JourneyInput<'_>) -> u8 {
        let nav_direction = match i.nav {
            "turn left" => 1,
            "turn right" => 2,
            _ => 0,
        };
        let distance = i
            .distance
            .filter(|d| d.is_finite() && (0. ..=250.).contains(d));
        let nav_changed = i.nav != self.nav;
        let new_cue = nav_changed
            || matches!((distance, self.distance), (Some(d), Some(old)) if d > old + 10.);
        self.nav = i.nav.to_owned();
        self.distance = distance;
        if self.direction == 0 && nav_changed {
            self.consumed = false;
        }
        if self.direction != 0 && new_cue && nav_direction != 0 {
            self.pending = Some((nav_direction, i.now));
        }
        if !i.permitted {
            self.consumed |= self.direction != 0;
            self.direction = 0;
            self.pending = None;
            return 0;
        }
        if self.direction != 0 {
            let complete = self.geometry(i.car, i.pose);
            let sign = if self.direction == 1 { 1. } else { -1. };
            let car = i.car;
            let driver_abort = self.peak_angle >= 30.
                && car.v_ego >= 1. / 3.6
                && car.steering_pressed
                && car.steering_torque * sign < 0.
                && car.steering_angle_deg * sign < 0.;
            let nav_exit = nav_direction == 0
                && !matches!(
                    i.nav,
                    "" | "none"
                        | "none prepare"
                        | "straight"
                        | "straight prepare"
                        | "turn left prepare"
                        | "turn right prepare"
                );
            if driver_abort || nav_exit || !(0. ..60.).contains(&(i.now - self.started)) {
                self.direction = 0;
                self.pending = None;
                self.consumed = true;
            } else if complete {
                self.direction = 0;
                self.consumed = true;
                self.peak_angle = 0.;
                self.rotation = 0.;
                self.entered = false;
                if let Some((pending, at)) = self.pending {
                    if (0. ..=3.).contains(&(i.now - at)) && i.desire == pending && i.lateral_active
                    {
                        // A same-side handoff keeps the outer request's 60 s bound.
                        self.direction = pending;
                        self.pose = i.pose;
                    }
                }
                self.pending = None;
            } else if nav_direction == 0 && !self.entered {
                self.direction = 0;
                self.consumed = true;
            }
        } else if !self.consumed && i.lateral_active && i.desire != 0 && i.desire == nav_direction {
            self.direction = i.desire;
            self.started = i.now;
            self.peak_angle = 0.;
            self.rotation = 0.;
            self.entered = false;
            self.pose = i.pose;
            self.pending = None;
        }
        self.direction
    }
}
