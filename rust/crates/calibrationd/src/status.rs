use crate::{types::INPUTS_NEEDED, Calibrator, Error, Seed, Status};

impl Calibrator {
    pub fn update_status(&mut self) -> Result<(), Error> {
        let indices = self.valid_indices();
        if indices.is_empty() {
            self.spread = vec![0.0; 3];
        } else {
            let count = f64::from(
                u32::try_from(indices.len()).map_err(|_| Error::Contract("history too long"))?,
            );
            self.rpy.fill(0.0);
            self.wide.fill(0.0);
            self.height = 0.0;
            let first = &self.rpys[usize::from(indices[0])];
            let mut maximum = first.clone();
            let mut minimum = first.clone();
            for index in indices {
                let index = usize::from(index);
                for (axis, value) in self.rpys[index].iter().copied().enumerate() {
                    self.rpy[axis] += value;
                    // NumPy reductions propagate NaNs instead of f64::min/max's single-NaN rule.
                    maximum[axis] = if value.is_nan() {
                        value
                    } else if maximum[axis].is_nan() {
                        maximum[axis]
                    } else {
                        maximum[axis].max(value)
                    };
                    minimum[axis] = if value.is_nan() {
                        value
                    } else if minimum[axis].is_nan() {
                        minimum[axis]
                    } else {
                        minimum[axis].min(value)
                    };
                }
                for axis in 0..3 {
                    self.wide[axis] += self.wides[index][axis];
                }
                self.height += self.heights[index];
            }
            self.rpy.iter_mut().for_each(|value| *value /= count);
            self.wide.iter_mut().for_each(|value| *value /= count);
            self.height /= count;
            self.spread = maximum
                .iter()
                .zip(minimum)
                .map(|(max, min)| (max - min).abs())
                .collect();
        }
        self.status = if self.valid_blocks < INPUTS_NEEDED {
            if self.status == Status::Recalibrating {
                Status::Recalibrating
            } else {
                Status::Uncalibrated
            }
        } else if self.limits.valid(&self.rpy)? {
            Status::Calibrated
        } else {
            Status::Invalid
        };
        let pitch_spread = *self
            .spread
            .get(1)
            .ok_or(Error::Contract("pitch spread missing"))?;
        let spread_high = pitch_spread > 4.0_f64.to_radians()
            || *self
                .spread
                .get(2)
                .ok_or(Error::Contract("yaw spread missing"))?
                > 2.0_f64.to_radians();
        if spread_high && self.status == Status::Calibrated {
            let previous = usize::from((self.block_idx + 49) % 50);
            self.reset(
                Seed {
                    rpy: self.rpys[previous].clone(),
                    valid_blocks: 1,
                    ..Seed::default()
                },
                Some(self.rpy.clone()),
            )?;
            self.status = Status::Recalibrating;
        }
        Ok(())
    }
}
