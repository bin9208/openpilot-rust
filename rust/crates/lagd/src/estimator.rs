use crate::{
    blocks::BlockAverage,
    estimate::{Delay, Estimate},
    motion::Motion,
    points::Points,
    settings::Settings,
    smoothing, Error,
};
use num_traits::ToPrimitive;
#[derive(Debug, serde::Serialize)]
pub struct Evaluation {
    pub estimate: Estimate,
    pub valid: bool,
    pub accepted: bool,
}
pub struct Estimator {
    pub settings: Settings,
    pub initial_lag: f64,
    pub motion: Motion,
    pub points: Points,
    pub blocks: BlockAverage,
    pub last_estimate_t: f64,
    needed: usize,
    delay: Delay,
}
impl Estimator {
    pub fn new(settings: Settings, actuator_delay: f64) -> Result<Self, Error> {
        if settings.dt <= 0. || !settings.dt.is_finite() {
            return Err(Error::Contract("positive finite dt required"));
        }
        let count = (settings.window_sec / settings.dt)
            .to_usize()
            .ok_or(Error::Contract("moving window size"))?;
        let needed = (settings.okay_window_sec / settings.dt)
            .to_usize()
            .ok_or(Error::Contract("valid window size"))?;
        let initial_lag = actuator_delay + 0.2;
        let blocks =
            BlockAverage::new(settings.block_count, settings.block_size, (initial_lag, 0))?;
        let delay = Delay::new(count, settings.dt, (0.15, 1.))?;
        Ok(Self {
            settings,
            initial_lag,
            motion: Motion::default(),
            points: Points::new(count),
            blocks,
            last_estimate_t: 0.,
            needed,
            delay,
        })
    }
    pub fn reset(&mut self, lag: f64, valid_blocks: i32) -> Result<(), Error> {
        self.points = Points::new(self.points.rows.len());
        self.blocks = BlockAverage::new(
            self.settings.block_count,
            self.settings.block_size,
            (lag, valid_blocks),
        )?;
        Ok(())
    }
    pub fn update_points(&mut self) {
        self.points.update(self.motion.point(&self.settings));
    }
    pub fn update_estimate(&mut self) -> Result<Option<Evaluation>, Error> {
        if self.points.rows.len() < self.needed {
            return Ok(None);
        }
        let desired: Vec<_> = self.points.rows.iter().map(|point| point.desired).collect();
        let actual: Vec<_> = self.points.rows.iter().map(|point| point.actual).collect();
        let mask: Vec<_> = self.points.rows.iter().map(|point| point.okay).collect();
        let range = if actual.iter().any(|value| value.is_nan()) {
            f64::NAN
        } else {
            actual.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                - actual.iter().copied().fold(f64::INFINITY, f64::min)
        };
        let mut valid = self.points.okay() >= self.needed && range >= 0.5;
        if self.last_estimate_t != 0.
            && self
                .points
                .rows
                .front()
                .is_some_and(|point| point.time <= self.last_estimate_t)
        {
            let distance = self
                .points
                .rows
                .iter()
                .rev()
                .position(|point| point.time <= self.last_estimate_t)
                .ok_or(Error::Contract("new sample boundary"))?;
            valid = valid
                && distance != 0
                && self
                    .points
                    .rows
                    .iter()
                    .skip(self.points.rows.len() - distance)
                    .any(|point| point.okay);
        }
        let desired = smoothing::masked(&desired, &mask, (5, 1.))?;
        let actual = smoothing::masked(&actual, &mask, (5, 1.))?;
        let estimate = self.delay.estimate(&desired, &actual, &mask)?;
        // Written as the source rejection test so NaN comparisons are not silently inverted.
        let accepted = !(estimate.correlation < self.settings.min_ncc
            || estimate.confidence < self.settings.min_confidence
            || !valid);
        if accepted {
            self.blocks.update(estimate.delay)?;
            self.last_estimate_t = self.motion.t;
        }
        Ok(Some(Evaluation {
            estimate,
            valid,
            accepted,
        }))
    }
}
