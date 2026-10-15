//! Final signal constraints, applied after coasting and before publication.
use super::PrepareAccel;
use openpilot_control_policy::math::{maximum, minimum};
use serde::{Deserialize, Serialize};
#[derive(Debug, Deserialize)]
pub struct Input {
    pub now: f64,
    pub dt: f64,
    pub prepare_active: bool,
    pub control_active: bool,
    pub hold: bool,
    pub reset: bool,
    pub gas: bool,
    pub brake: bool,
    pub acc: bool,
    pub lead_one: bool,
    pub lead_two: bool,
    #[serde(default)]
    pub lead_one_distance: f64,
    #[serde(default)]
    pub lead_two_distance: f64,
    #[serde(default)]
    pub soft_hold_active: i32,
    pub ego_speed: f64,
    pub ego_accel: f64,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct Output {
    pub should_stop: bool,
    pub speed: f64,
    pub accel: f64,
    pub jerk: f64,
    pub a_desired: f64,
    pub coasting: f64,
}
#[derive(Debug, Default)]
pub struct Constraints {
    time: Option<f64>,
    previous: Option<f64>,
    accel: PrepareAccel,
}
impl Constraints {
    pub fn apply(&mut self, input: &Input, output: &mut Output) {
        let elapsed = self.time.map_or_else(
            || input.now - (input.now - input.dt),
            |last| input.now - last,
        );
        self.time = Some(input.now);
        let active = input.prepare_active
            && input.control_active
            && !input.reset
            && !input.gas
            && !input.brake
            && 5. / 3.6 < input.ego_speed
            && input.ego_speed < 82. / 3.6
            && input.soft_hold_active == 0
            && 0. < elapsed
            && elapsed <= 0.2
            && !output.should_stop
            && input.acc
            && !super::prepare_lead_blocked(&crate::radar::Radar {
                lead_one: crate::lead::Lead {
                    status: input.lead_one,
                    d_rel: input.lead_one_distance,
                    ..Default::default()
                },
                lead_two: crate::lead::Lead {
                    status: input.lead_two,
                    d_rel: input.lead_two_distance,
                    ..Default::default()
                },
                ..Default::default()
            });
        let previous = self.previous.unwrap_or(input.ego_accel);
        let uncapped = output.accel;
        output.accel = self.accel.apply(output.accel, previous, elapsed, active);
        self.previous = Some(output.accel);
        if active {
            output.coasting = 0.;
            output.speed = minimum(
                output.speed,
                maximum(0., input.ego_speed + output.accel * input.dt),
            );
            output.a_desired = minimum(output.a_desired, output.accel);
            if output.accel < uncapped {
                output.jerk = minimum(output.jerk, (output.accel - previous) / elapsed);
            }
        }
        if input.hold {
            output.should_stop = true;
            output.speed = 0.;
            output.accel = minimum(output.accel, 0.);
        }
    }
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"time":self.time,"previous":self.previous,"cap":self.accel.cap})
    }
}
pub fn action(hold: bool, prepare: bool, control_active: bool, green: bool) -> u8 {
    if hold {
        1
    } else if prepare && control_active {
        2
    } else if green {
        3
    } else {
        0
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stopped_veto_keeps_stronger_braking_and_preparation_clears_coasting() {
        let mut constraints = Constraints::default();
        let mut input = Input {
            now: 10.,
            dt: 0.05,
            prepare_active: true,
            control_active: true,
            hold: false,
            reset: false,
            gas: false,
            brake: false,
            acc: true,
            lead_one: false,
            lead_two: false,
            lead_one_distance: 0.,
            lead_two_distance: 0.,
            soft_hold_active: 0,
            ego_speed: 10.,
            ego_accel: 0.,
        };
        let mut output = Output {
            should_stop: false,
            speed: 11.,
            accel: 0.,
            jerk: 0.,
            a_desired: 0.,
            coasting: 10.,
        };
        constraints.apply(&input, &mut output);
        assert!(output.accel < 0.);
        assert_eq!(output.coasting, 0.);
        assert!(!output.should_stop);
        input.now += 0.05;
        input.hold = true;
        output.accel = -2.;
        constraints.apply(&input, &mut output);
        assert_eq!(output.accel, -2.);
        assert_eq!(output.speed, 0.);
        assert!(output.should_stop);
        assert_eq!(action(true, true, true, true), 1);
        assert_eq!(action(false, true, true, true), 2);
    }
}
