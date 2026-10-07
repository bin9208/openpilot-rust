use super::SensorKind;
use crate::arithmetic::{multiply_add32, multiply_add64};

#[derive(Clone, Copy, Debug)]
pub struct ExposureScore {
    pub desired_ev: f32,
    pub time: i32,
    pub gain_index: i32,
    pub gain: f32,
    pub previous_gain_index: i32,
}

impl SensorKind {
    // C++ mixes float arithmetic with double literals in the final score term.
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "preserve source C++ integer/float conversions and final double-to-float rounding"
    )]
    pub fn exposure_score(self, candidate: ExposureScore) -> f32 {
        let config = self.config();
        let mut score = multiply_add32(
            -(candidate.time as f32),
            candidate.gain,
            candidate.desired_ev,
        )
        .abs();
        score *= match self {
            Self::Ar0231 => 10.0,
            Self::Ox03c10 | Self::Os04c10 => 1.0,
        };
        let cost = if candidate.gain_index > config.analog_gain_rec_idx {
            config.analog_gain_cost_high
        } else {
            config.analog_gain_cost_low
        };
        score = multiply_add32(
            (candidate.gain_index - config.analog_gain_rec_idx).abs() as f32,
            cost,
            score,
        );
        let change = (candidate.gain_index - candidate.previous_gain_index).abs();
        let multiplier = match self {
            Self::Ar0231 => {
                return (f64::from(score) + f64::from(change) * (f64::from(score) + 1.0) / 10.0)
                    as f32
            }
            Self::Ox03c10 => 5.0,
            Self::Os04c10 => 3.0,
        };
        let weight = (1 - config.analog_gain_cost_delta)
            + config.analog_gain_cost_delta * (candidate.gain_index - config.analog_gain_min_idx)
                / (config.analog_gain_max_idx - config.analog_gain_min_idx);
        multiply_add64(f64::from(weight * change), multiplier, f64::from(score)) as f32
    }
}
