//! Bounded braking comfort preference from final longitudinal_braking_gap.py.
use openpilot_control_policy::math::{maximum, minimum};

#[derive(Clone, Copy)]
pub struct Observation {
    pub enabled: bool,
    pub track_id: i32,
    pub dt: f64,
    pub distance: f64,
    pub relative_speed: f64,
}

#[derive(Clone, Copy)]
pub struct ComfortInput<'a, const N: usize> {
    pub dt: f64,
    pub enabled: bool,
    pub base_tf: f64,
    pub ego_speeds: &'a [f64; N],
    pub lead_speeds: &'a [f64; N],
    pub lead_accel: f64,
    pub physical_gaps: &'a [f64; N],
    pub danger_margins: &'a [f64; N],
    pub times: &'a [f64; N],
    pub stop_distance: f64,
}

#[derive(Default, Debug)]
pub struct LeadComfortState {
    key: Option<i32>,
    age: f64,
    distance: f64,
    relative_speed: f64,
    pub relief: f64,
}

pub fn has_reserve<const N: usize>(
    speeds: &[f64; N],
    gaps: &[f64; N],
    danger: &[f64; N],
    times: &[f64; N],
    stop: f64,
) -> bool {
    speeds
        .iter()
        .chain(gaps)
        .chain(danger)
        .all(|v| v.is_finite())
        && (0..N)
            .filter(|i| times[*i] <= 3.)
            .all(|i| danger[i] > 3. && gaps[i] - stop - 0.7 * speeds[i] > 2.)
}

impl LeadComfortState {
    pub fn update(&mut self, input: Observation) {
        let valid = input.enabled
            && input.track_id >= 0
            && [input.dt, input.distance, input.relative_speed]
                .iter()
                .all(|v| v.is_finite())
            && input.dt > 0.
            && input.dt <= 0.2
            && input.distance > 0.;
        if !valid {
            *self = Self::default();
            return;
        }
        if self.key != Some(input.track_id)
            || (input.distance - self.distance - self.relative_speed * input.dt).abs() > 3.
        {
            *self = Self::default();
            self.key = Some(input.track_id);
        }
        self.age += input.dt;
        self.distance = input.distance;
        self.relative_speed = input.relative_speed;
    }

    pub fn comfort_relief<const N: usize>(&mut self, input: ComfortInput<'_, N>) -> [f64; N] {
        const {
            assert!(N > 0);
        }
        let speed = input.ego_speeds[0];
        let eligible = input.enabled
            && self.age >= 1.
            && input.base_tf > 0.7
            && speed > 5.
            && input.lead_speeds[0] > 2.
            && input.lead_accel < -0.3
            && self.relative_speed > -2.5
            && self.relative_speed < 0.
            && self.distance - input.stop_distance > speed * (input.base_tf + 0.2)
            && has_reserve(
                input.ego_speeds,
                input.physical_gaps,
                input.danger_margins,
                input.times,
                input.stop_distance,
            );
        self.relief = if eligible {
            minimum(1., self.relief + input.dt / 0.5)
        } else {
            0.
        };
        if !eligible {
            return [0.; N];
        }
        input
            .ego_speeds
            .map(|v| self.relief * minimum(0.4, maximum(0., input.base_tf - 0.7)) * maximum(v, 0.))
    }
}
