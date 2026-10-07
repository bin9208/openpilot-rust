use super::{GapLead, Input, Plan, Reason, Tracker};
use crate::{lane_change_gap::check_time, Error};
use openpilot_control_policy::math::{interp, maximum, minimum};

impl Tracker {
    pub(super) fn check_path(
        &mut self,
        input: &Input,
        lead: GapLead,
        base: Plan,
    ) -> Result<Plan, Error> {
        let front_y =
            self.ego_y + lead.distance * self.heading.sin() - lead.lateral * self.heading.cos();
        self.history.push_back((input.now, self.ego_y, front_y));
        while self
            .history
            .front()
            .is_some_and(|sample| input.now - sample.0 > 0.5)
        {
            self.history.pop_front();
        }
        let Some(&(old_t, old_y, old_front_y)) = self.history.front() else {
            return Err(Error::Contract("missing lane-change history"));
        };
        if self.history.len() < 6 || input.now - old_t < 0.25 {
            return Ok(Plan {
                reason: Reason::ConfirmMotion,
                ..base
            });
        }
        let direction = f64::from(input.direction);
        let progress = direction * (self.ego_y - old_y);
        if direction * self.ego_y < 0.25
            || progress < 0.10
            || progress / (input.now - old_t) < 0.25
            || (front_y - old_front_y).abs() > 0.10
            || input.now - self.targets_since < 0.30
        {
            return Ok(Plan {
                reason: Reason::UnconfirmedMotion,
                ..base
            });
        }
        let times = &input.path_t;
        if times.len() < 6
            || times.len() != input.path_x.len()
            || times.len() != input.path_y.len()
            || !times
                .iter()
                .chain(&input.path_x)
                .chain(&input.path_y)
                .all(|v| v.is_finite())
            || times.windows(2).any(|pair| pair[1] <= pair[0])
            || times[0] > 0.1
            || times[times.len() - 1] < 3.
        {
            return Ok(Plan {
                reason: Reason::InvalidPath,
                ..base
            });
        }
        let rate = progress / (input.now - old_t);
        let mut first = None;
        let mut reentry = false;
        let mut previous_y = None;
        for sample in 0..61 {
            let time = check_time(sample);
            let x = interp(time, times, &input.path_x)?;
            let y = interp(time, times, &input.path_y)?;
            let predicted = self.ego_y + x * self.heading.sin() + y * self.heading.cos();
            let separation = minimum(
                direction * (predicted - front_y),
                direction * (self.ego_y - front_y) + rate * time,
            );
            let clear = separation > 2.5 + 0.2 * time;
            if first.is_none() && clear && (0.35..=2.5).contains(&time) {
                first = Some(time);
            }
            if first.is_some() && !clear {
                reentry = true;
            }
            if previous_y.is_some_and(|previous| direction * (predicted - previous) < -0.02) {
                reentry = true;
            }
            previous_y = Some(predicted);
        }
        let Some(clearance) = first else {
            return Ok(Plan {
                reason: Reason::NoClearance,
                ..base
            });
        };
        if reentry {
            return Ok(Plan {
                reason: Reason::PathReentry,
                ..base
            });
        }
        let confidence = minimum(
            1.,
            maximum(0., (input.now - self.targets_since - 0.30) / 0.30),
        );
        Ok(Plan {
            clearance,
            confidence,
            reason: Reason::ConfirmedDeparture,
            ..base
        })
    }
}
