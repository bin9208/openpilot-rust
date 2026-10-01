use crate::{
    math::{clip, interp, minimum, sign},
    Error,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Gain {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
}
impl Gain {
    pub fn constant(value: f64) -> Self {
        Self {
            x: vec![0.],
            y: vec![value],
        }
    }
    fn at(&self, speed: f64) -> Result<f64, Error> {
        interp(speed, &self.x, &self.y)
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Gains {
    pub p: Gain,
    pub i: Gain,
    pub d: Gain,
    pub f: f64,
}
impl Gains {
    pub fn constants(p: f64, i: f64, f: f64) -> Self {
        Self {
            p: Gain::constant(p),
            i: Gain::constant(i),
            d: Gain::constant(0.),
            f,
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Step {
    pub error: f64,
    pub error_rate: f64,
    pub speed: f64,
    pub driver_override: bool,
    pub feedforward: f64,
    pub freeze: bool,
}
impl Step {
    pub fn new(error: f64, speed: f64, feedforward: f64) -> Self {
        Self {
            error,
            error_rate: 0.,
            speed,
            feedforward,
            driver_override: false,
            freeze: false,
        }
    }
}
#[derive(Debug)]
pub struct Multiplicative {
    pub previous_override: bool,
    pub factor: f64,
    pub min_command: f64,
    pub reduction_time: f64,
}
#[derive(Debug)]
pub struct Pid {
    pub gains: Gains,
    pub low: f64,
    pub high: f64,
    pub rate: f64,
    pub p: f64,
    pub i: f64,
    pub d: f64,
    pub f: f64,
    pub control: f64,
    pub multiplicative: Option<Multiplicative>,
}
impl Pid {
    pub fn new(gains: Gains, limits: [f64; 2]) -> Self {
        Self {
            gains,
            low: limits[0],
            high: limits[1],
            rate: 100.,
            p: 0.,
            i: 0.,
            d: 0.,
            f: 0.,
            control: 0.,
            multiplicative: None,
        }
    }
    pub fn reset(&mut self) {
        self.p = 0.;
        self.i = 0.;
        self.d = 0.;
        self.f = 0.;
        self.control = 0.;
    }
    pub fn update(&mut self, input: Step) -> Result<f64, Error> {
        self.p = input.error * self.gains.p.at(input.speed)?;
        self.f = input.feedforward * self.gains.f;
        self.d = input.error_rate * self.gains.d.at(input.speed)?;
        if input.driver_override {
            match &mut self.multiplicative {
                Some(unwind) => {
                    if !unwind.previous_override {
                        unwind.factor = if unwind.reduction_time <= 0. {
                            1.
                        } else if self.i.abs() <= unwind.min_command {
                            0.
                        } else {
                            let steps = (unwind.reduction_time * self.rate).trunc().max(1.);
                            minimum((unwind.min_command / self.i.abs()).powf(1. / steps), 1.)
                        };
                    }
                    self.i *= unwind.factor;
                    if self.i.abs() < unwind.min_command {
                        self.i = 0.;
                    }
                }
                None => self.i -= (0.3 / self.rate) * sign(self.i),
            }
        } else if !input.freeze {
            self.i += input.error * self.gains.i.at(input.speed)? * (1. / self.rate);
            let control = clip(self.p + self.d + self.f, self.low, self.high);
            self.i = clip(self.i, self.low - control, self.high - control);
        }
        self.control = clip(self.p + self.i + self.d + self.f, self.low, self.high);
        if let Some(unwind) = &mut self.multiplicative {
            unwind.previous_override = input.driver_override;
        }
        Ok(self.control)
    }
}
