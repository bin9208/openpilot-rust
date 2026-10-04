use crate::{window::Window, Error};
use openpilot_control_policy::math::{interp, maximum};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelLeadInput {
    pub stop_active: bool,
    pub allow_confirmation: bool,
    pub active_lead: bool,
    pub stop_distance: f64,
    pub lead_probability: f64,
    pub lead_distance: f64,
    pub lead_velocity: f64,
    pub lead_x_std: f64,
    pub lead_y_std: f64,
    pub lead_v_std: f64,
}

#[derive(Debug, Default)]
pub struct TrafficStopModelLeadMatcher {
    distances: Window<5>,
    velocities: Window<5>,
    match_count: u8,
    confirmed: bool,
}

impl TrafficStopModelLeadMatcher {
    fn clear_pending(&mut self) {
        self.distances.clear();
        self.velocities.clear();
        self.match_count = 0;
    }

    pub fn reset(&mut self) {
        self.clear_pending();
        self.confirmed = false;
    }

    pub fn update(&mut self, input: &ModelLeadInput) -> Result<f64, Error> {
        if !input.stop_active || input.active_lead {
            self.reset();
            return Ok(0.);
        }
        if self.confirmed {
            return Ok(2.);
        }
        if !input.allow_confirmation
            || ![
                input.stop_distance,
                input.lead_probability,
                input.lead_distance,
                input.lead_velocity,
                input.lead_x_std,
                input.lead_y_std,
                input.lead_v_std,
            ]
            .iter()
            .all(|value| value.is_finite())
        {
            self.clear_pending();
            return Ok(0.);
        }
        self.distances.push(input.lead_distance);
        self.velocities.push(input.lead_velocity);
        let distance = self.distances.median()?;
        let velocity = self.velocities.median()?;
        let gap = distance - input.stop_distance;
        let valid = input.lead_probability >= 0.90
            && (4. ..=80.).contains(&distance)
            && (0. ..=3.).contains(&gap)
            && velocity.abs() <= 2.
            && (0. ..=5.).contains(&input.lead_x_std)
            && (0. ..=0.75).contains(&input.lead_y_std)
            && (0. ..=1.5).contains(&input.lead_v_std);
        if valid {
            self.match_count += 1;
            self.confirmed = self.match_count >= 5;
        } else {
            self.clear_pending();
        }
        Ok(if self.confirmed { 2. } else { 0. })
    }
}

pub fn entry_allowed(steering_angle: f64) -> bool {
    steering_angle.abs() < 50.
}

pub fn distance_adjust(configured: f64, speed: f64, model_lead_offset: f64) -> f64 {
    if model_lead_offset.is_finite() && model_lead_offset > 0. {
        model_lead_offset
    } else if speed > 0.1 {
        configured
    } else {
        -2.
    }
}

pub fn obstacle_distance(stop: f64, cruise: f64, adjustment: f64) -> Result<f64, Error> {
    let signal = maximum(0., stop + adjustment);
    let cruise = maximum(0., cruise);
    if 50. < signal && signal < cruise {
        let release = interp(signal, &[50., cruise], &[1., 0.])?;
        Ok(cruise + release * (signal - cruise))
    } else {
        Ok(signal)
    }
}
