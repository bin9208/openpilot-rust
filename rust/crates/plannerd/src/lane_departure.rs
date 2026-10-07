use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prediction {
    pub left_probability: f64,
    pub right_probability: f64,
    pub left_visibility: f64,
    pub right_visibility: f64,
    pub left_y: f64,
    pub right_y: f64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaneDepartureInput {
    pub frame: u64,
    pub speed: f64,
    pub left_blinker: bool,
    pub right_blinker: bool,
    pub lateral_active: bool,
    pub prediction: Option<Prediction>,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct Warning {
    pub left: bool,
    pub right: bool,
}

#[derive(Debug, Default)]
pub struct LaneDeparture {
    last_blinker_frame: u64,
    warning: Warning,
}

impl LaneDeparture {
    pub fn update(&mut self, input: &LaneDepartureInput) -> Warning {
        if input.left_blinker || input.right_blinker {
            self.last_blinker_frame = input.frame;
        }
        let recent_blinker = input.frame.saturating_sub(self.last_blinker_frame) < 500;
        self.warning = match input.prediction {
            Some(prediction)
                if input.speed > 31. * (1.609344 * (1. / 3.6))
                    && !recent_blinker
                    && !input.lateral_active =>
            {
                Warning {
                    left: prediction.left_probability > 0.1
                        && prediction.left_visibility > 0.5
                        && prediction.left_y > -(1.08 + 0.04),
                    right: prediction.right_probability > 0.1
                        && prediction.right_visibility > 0.5
                        && prediction.right_y < 1.08 - 0.04,
                }
            }
            Some(_) | None => Warning::default(),
        };
        self.warning
    }
}

impl LaneDeparture {
    pub fn update_model(
        &mut self,
        frame: u64,
        model: &crate::model::Model,
        car: &crate::car_state::CarState,
        lateral_active: bool,
    ) -> Result<Warning, crate::Error> {
        if car.left_blinker || car.right_blinker {
            self.last_blinker_frame = frame;
        }
        let allowed = car.v_ego > 31. * (1.609344 * (1. / 3.6))
            && frame.saturating_sub(self.last_blinker_frame) >= 500
            && !lateral_active;
        let prediction = if !model.meta.desire_prediction.is_empty() && allowed {
            let value = |values: &[f64], index| {
                values
                    .get(index)
                    .copied()
                    .ok_or(crate::Error::Contract("missing lane-departure value"))
            };
            let right = value(&model.lane_line_probs, 2)?;
            let left = value(&model.lane_line_probs, 1)?;
            let left_probability = value(&model.meta.desire_prediction, 3)?;
            let right_probability = value(&model.meta.desire_prediction, 4)?;
            let lane_y = |index: usize| -> Result<f64, crate::Error> {
                let lane = model
                    .lane_lines
                    .get(index)
                    .ok_or(crate::Error::Contract("missing departure lane"))?;
                value(&lane.y, 0)
            };
            Some(Prediction {
                left_probability,
                right_probability,
                left_visibility: left,
                right_visibility: right,
                left_y: if left > 0.5 { lane_y(1)? } else { 0. },
                right_y: if right > 0.5 { lane_y(2)? } else { 0. },
            })
        } else {
            None
        };
        Ok(self.update(&LaneDepartureInput {
            frame,
            speed: car.v_ego,
            left_blinker: car.left_blinker,
            right_blinker: car.right_blinker,
            lateral_active,
            prediction,
        }))
    }
}
