use crate::{
    orientation,
    types::{BLOCK_SIZE, INPUTS_NEEDED, INPUTS_WANTED, MIN_SPEED},
    Calibrator, Error, Odometry, Update,
};

impl Calibrator {
    pub fn update(&mut self, input: &Odometry) -> Result<Update, Error> {
        self.old_weight = (self.old_weight - 0.1).max(0.0);
        let x = *input
            .trans
            .first()
            .ok_or(Error::Contract("translation x missing"))?;
        let straight_fast = self.v_ego > MIN_SPEED
            && x > MIN_SPEED
            && input
                .rot
                .get(2)
                .ok_or(Error::Contract("rotation z missing"))?
                .abs()
                < 2.0_f64.to_radians();
        let certain_rpy = input
            .trans_std
            .get(1)
            .ok_or(Error::Contract("translation deviation y missing"))?
            .atan2(x)
            < 0.25_f64.to_radians();
        let certain_height = input.road_std.len() != 3 || input.road_std[2] < (-3.5_f64).exp();
        if !(straight_fast && (certain_rpy && certain_height || self.valid_blocks < INPUTS_NEEDED))
        {
            return Ok(Update {
                rpy: None,
                persist: false,
            });
        }
        let observed = [
            0.0,
            -input
                .trans
                .get(2)
                .ok_or(Error::Contract("translation z missing"))?
                .atan2(x),
            input
                .trans
                .get(1)
                .ok_or(Error::Contract("translation y missing"))?
                .atan2(x),
        ];
        let smooth: [f64; 3] = self
            .smooth_rpy()?
            .try_into()
            .map_err(|_| Error::Contract("RPY must contain three angles"))?;
        let rpy = self.limits.clip(orientation::compose(smooth, observed));
        let wide: [f64; 3] = input.wide.as_slice().try_into().unwrap_or([0.0; 3]);
        let height = if input.road.len() == 3 {
            input.road[2]
        } else {
            1.22
        };
        let index = usize::from(self.block_idx);
        let previous = f64::from(self.idx);
        let incoming = f64::from(BLOCK_SIZE - self.idx);
        for axis in 0..3 {
            self.rpys[index][axis] =
                (previous * self.rpys[index][axis] + incoming * rpy[axis]) / f64::from(BLOCK_SIZE);
            self.wides[index][axis] = (previous * self.wides[index][axis] + incoming * wide[axis])
                / f64::from(BLOCK_SIZE);
        }
        self.heights[index] =
            (previous * self.heights[index] + incoming * height) / f64::from(BLOCK_SIZE);
        self.idx = (self.idx + 1) % BLOCK_SIZE;
        if self.idx == 0 {
            self.block_idx += 1;
            self.valid_blocks = self.block_idx.max(self.valid_blocks);
            self.block_idx %= INPUTS_WANTED;
        }
        self.update_status()?;
        Ok(Update {
            rpy: Some(rpy),
            persist: self.idx == 0 && self.block_idx % 10 == 5,
        })
    }
}
