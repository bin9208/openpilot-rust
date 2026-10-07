use super::{math, LanePathInput, LanePlanner};
use crate::{parameters::Parameters, Error};
use openpilot_control_policy::math::{clip, interp, maximum, minimum, sign};

impl LanePlanner {
    pub fn apply(
        &mut self,
        input: LanePathInput<'_>,
        parameters: &mut impl Parameters,
        path: &mut [[f64; 3]; 33],
    ) -> Result<bool, Error> {
        let width = math::paired(&self.right_y, &self.left_y, |a, b| a - b)?;
        let mut modifiers = [0.; 3];
        for (index, time) in [0., 1.5, 3.].into_iter().enumerate() {
            let width_at_time = interp(time * (input.speed + 7.), &self.x, &width)?;
            modifiers[index] = interp(width_at_time, &[4.5, 6.], &[1., 0.])?;
        }
        let modifier = minimum(minimum(modifiers[0], modifiers[1]), modifiers[2]);
        let left =
            self.probabilities[0] * modifier * interp(self.deviations[0], &[0.15, 0.3], &[1., 0.])?;
        let right =
            self.probabilities[1] * modifier * interp(self.deviations[1], &[0.15, 0.3], &[1., 0.])?;
        let current_width = (math::value(&self.right_y, 0)? - math::value(&self.left_y, 0)?).abs();
        let both = left > 0.5 && right > 0.5 && input.change_multiplier > 0.5;
        if both {
            self.width_last = self.width_filter.update(current_width);
        } else {
            self.width_filter.update(self.width_last);
        }
        self.lane_width = self.width_filter.value();
        let clipped_width = minimum(4., self.lane_width);
        let from_left: Vec<_> = self.left_y.iter().map(|y| y + clipped_width / 2.).collect();
        let from_right: Vec<_> = self
            .right_y
            .iter()
            .map(|y| y - clipped_width / 2.)
            .collect();
        self.d_probability = if both { 1. } else { maximum(left, right) };
        self.side_widths = input.side_widths;
        for (filter, width) in self.side_filters.iter_mut().zip(input.side_widths) {
            if width > 0. {
                filter.update(width);
            }
        }
        let left_width = self.side_filters[0].value();
        let right_width = self.side_filters[1].value();
        let adjustment = f64::from(parameters.integer("AdjustLaneOffset")?) * 0.01;
        let curve_offset = interp(input.curve_speed.abs(), &[50., 200.], &[adjustment, 0.])?
            * sign(input.curve_speed);
        let lane_offset =
            if (left_width > 2.2 && right_width > 2.2) || (left_width < 2. && right_width < 2.) {
                0.
            } else if left_width > right_width {
                interp(self.lane_width, &[2.5, 2.9], &[0., adjustment])?
            } else {
                interp(self.lane_width, &[2.5, 2.9], &[0., -adjustment])?
            };
        let lane_y = if self.lane_width < 2.5 {
            if right > 0.5 && right_width < left_width {
                from_right
            } else if left > 0.5 || left > right {
                from_left
            } else {
                from_right
            }
        } else if left > 0.7 && right > 0.7 {
            math::paired(&from_left, &from_right, |a, b| (a + b) / 2.)?
        } else {
            math::paired(&from_left, &from_right, |a, b| {
                (left * a + right * b) / (left + right + 0.0001)
            })?
        };
        let offset = if curve_offset * lane_offset < 0. {
            clip(curve_offset + lane_offset + 0., -0.4, 0.4)
        } else {
            let selected = if lane_offset.abs() > curve_offset.abs() {
                lane_offset
            } else {
                curve_offset
            };
            clip(selected + 0., -0.4, 0.4)
        };
        self.d_probability *= input.change_multiplier;
        let hold_offset = input.change_multiplier < 0.5;
        if !hold_offset {
            self.offset_filter
                .update(interp(self.d_probability, &[0., 0.3], &[0., offset])?);
        }
        self.d_probability *= interp(input.speed * 3.6, &[5., 10.], &[0., 1.])?;
        let time_adjustment = parameters.float("LatMpcInputOffset")? * 0.01;
        self.probability_count = if self.d_probability > 0.3 {
            self.probability_count.saturating_add(1)
        } else {
            0
        };
        let active = input.lane_mode && self.probability_count > 20;
        if active
            && self
                .times
                .first()
                .ok_or(Error::Contract("lane time vector empty"))?
                .is_finite()
        {
            if self.times.len() != lane_y.len() || ![1, 33].contains(&input.times.len()) {
                return Err(Error::Contract("lane interpolation dimensions"));
            }
            let (times, y): (Vec<_>, Vec<_>) = self
                .times
                .iter()
                .copied()
                .zip(lane_y)
                .filter(|(time, _)| time.is_finite())
                .unzip();
            for (index, point) in path.iter_mut().enumerate() {
                let query = math::value(input.times, index)? * (1. + time_adjustment);
                let lane = interp(query, &times, &y)?;
                point[1] = self.d_probability * lane + (1. - self.d_probability) * point[1];
            }
        }
        for point in path {
            point[1] += 0. + self.offset_filter.value();
        }
        self.offset_total = self.offset_filter.value();
        Ok(active)
    }
}
