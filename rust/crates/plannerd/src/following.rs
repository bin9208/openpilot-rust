use crate::{types::Personality, Error};
use openpilot_control_policy::math::{clip, interp, maximum, minimum};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GapParameters {
    pub gaps: [f64; 4],
    pub speed_factor: i32,
    pub decel_boost: f64,
}

impl Default for GapParameters {
    fn default() -> Self {
        Self {
            gaps: [1.1, 1.3, 1.45, 1.6],
            speed_factor: 10,
            decel_boost: 0.,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FollowingInput {
    pub personality: Personality,
    pub speed: f64,
    pub acceleration: f64,
    pub mode_factor: f64,
}

pub fn speed_factor(setting: i32, speed_kph: f64) -> f64 {
    let at_100 = f64::from(setting.clamp(10, 30)) * 0.1;
    1. + (at_100 - 1.) * maximum(0., speed_kph) / 100.
}

pub fn response_for_gap(common: i32, overrides: &[i32; 4], personality: Personality) -> i32 {
    let selected = overrides[personality.index()];
    if selected < 0 { common } else { selected }.clamp(0, 5)
}

pub fn ramp(target: f64, current: f64, decel_extra: f64) -> f64 {
    if target <= current {
        return target;
    }
    let rate = if decel_extra > 0.02 { 0.60 } else { 0.30 };
    minimum(target, current + rate * 0.05)
}

#[derive(Debug)]
pub struct FollowingTime {
    pub value: f64,
    pub jerk_factor: f64,
    pub decel_extra: f64,
    pub mode_factor: Option<f64>,
    pub decel_base: Option<f64>,
}

impl Default for FollowingTime {
    fn default() -> Self {
        Self {
            value: 1.5,
            jerk_factor: 1.,
            decel_extra: 0.,
            mode_factor: None,
            decel_base: None,
        }
    }
}

impl FollowingTime {
    pub fn update(
        &mut self,
        input: FollowingInput,
        parameters: &GapParameters,
    ) -> Result<f64, Error> {
        self.jerk_factor = input.personality.jerk_factor();
        let base = parameters.gaps[input.personality.index()];
        let target = base * speed_factor(parameters.speed_factor, input.speed * 3.6);
        let mode_factor = maximum(
            input.mode_factor,
            self.mode_factor.unwrap_or(input.mode_factor) - 0.05 * 0.05,
        );
        self.mode_factor = Some(mode_factor);
        let mode_target = target * mode_factor;
        let previous_base = self.decel_base.unwrap_or(mode_target);
        let decel_base = if input.acceleration <= -0.2 {
            maximum(mode_target, previous_base)
        } else {
            mode_target
        };
        self.decel_base = Some(decel_base);
        let requested_extra = interp(
            input.acceleration,
            &[-2.5, -1., -0.3, 0.],
            &[0.50, 0.25, 0.06, 0.],
        )? * parameters.decel_boost;
        self.decel_extra = maximum(requested_extra, self.decel_extra - 0.10 * 0.05);
        let adjusted = decel_base + self.decel_extra;
        let ceiling =
            maximum(mode_target, decel_base) * maximum(1., 1.) + maximum(0., self.decel_extra);
        let target = clip(adjusted, 0.3, maximum(0.3, ceiling));
        self.value = ramp(target, self.value, self.decel_extra);
        Ok(self.value)
    }
}
