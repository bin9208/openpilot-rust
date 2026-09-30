use crate::{types::INPUTS_WANTED, Error, Limits, Seed, Status};

pub struct Calibrator {
    pub limits: Limits,
    pub not_car: bool,
    pub status: Status,
    pub rpy: Vec<f64>,
    pub wide: [f64; 3],
    pub height: f64,
    pub valid_blocks: u8,
    pub idx: u8,
    pub block_idx: u8,
    pub v_ego: f64,
    pub spread: Vec<f64>,
    pub old_rpy: Vec<f64>,
    pub old_weight: f64,
    pub(crate) rpys: Vec<Vec<f64>>,
    pub(crate) wides: [[f64; 3]; 50],
    pub(crate) heights: [f64; 50],
}

impl Calibrator {
    pub fn new(limits: Limits, seed: Seed) -> Result<Self, Error> {
        let mut calibration = Self {
            limits,
            not_car: false,
            status: Status::Uncalibrated,
            rpy: Vec::new(),
            wide: [0.0; 3],
            height: 1.22,
            valid_blocks: 0,
            idx: 0,
            block_idx: 0,
            v_ego: 0.0,
            spread: vec![0.0; 3],
            old_rpy: vec![0.0; 3],
            old_weight: 0.0,
            rpys: Vec::new(),
            wides: [[0.0; 3]; 50],
            heights: [1.22; 50],
        };
        calibration.reset(seed, None)?;
        calibration.update_status()?;
        Ok(calibration)
    }

    pub fn reset(&mut self, seed: Seed, smooth_from: Option<Vec<f64>>) -> Result<(), Error> {
        if seed.valid_blocks > i32::from(INPUTS_WANTED) {
            return Err(Error::Contract("saved valid blocks exceed history"));
        }
        self.rpy = if seed.rpy.iter().all(|value| value.is_finite()) {
            seed.rpy
        } else {
            vec![0.0; 3]
        };
        self.height = if seed.height.len() == 1 && seed.height[0].is_finite() {
            seed.height[0]
        } else {
            1.22
        };
        self.wide = if seed.wide.iter().all(|value| value.is_finite()) {
            seed.wide.try_into().unwrap_or([0.0; 3])
        } else {
            [0.0; 3]
        };
        self.valid_blocks = u8::try_from(seed.valid_blocks.max(0))
            .map_err(|_| Error::Contract("invalid block count"))?;
        self.rpys = vec![self.rpy.clone(); usize::from(INPUTS_WANTED)];
        self.wides.fill(self.wide);
        self.heights.fill(self.height);
        self.idx = 0;
        self.block_idx = 0;
        self.v_ego = 0.0;
        match smooth_from {
            Some(rpy) => {
                self.old_rpy = rpy;
                self.old_weight = 1.0;
            }
            None => {
                self.old_rpy = vec![0.0; 3];
                self.old_weight = 0.0;
            }
        }
        Ok(())
    }

    pub fn valid_indices(&self) -> Vec<u8> {
        (0..self.block_idx)
            .chain(self.valid_blocks.min(self.block_idx + 1)..self.valid_blocks)
            .collect()
    }

    pub fn smooth_rpy(&self) -> Result<Vec<f64>, Error> {
        if self.old_weight <= 0.0 {
            return Ok(self.rpy.clone());
        }
        if self.old_rpy.len() != self.rpy.len() {
            return Err(Error::Contract("incompatible smoothing dimensions"));
        }
        Ok(self
            .old_rpy
            .iter()
            .zip(&self.rpy)
            .map(|(old, current)| self.old_weight * old + (1.0 - self.old_weight) * current)
            .collect())
    }

    pub fn frozen(&self, yaw_trim_deg: f64) -> bool {
        yaw_trim_deg.abs() > 1e-6 && self.status == Status::Calibrated
    }
}
