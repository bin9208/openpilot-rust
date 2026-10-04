use super::{CarrotPlanner, Input};
use crate::{
    lane_change_gap::{self, Plan, Reason},
    number::timestamp_seconds,
    Error,
};
use openpilot_cereal::log_capnp::LaneChangeState;

impl CarrotPlanner {
    pub(super) fn update_desire(&mut self, input: &Input<'_>) -> Result<(), Error> {
        let meta = &input.model.meta;
        let car = input.car;
        self.lane_change_active = matches!(
            meta.lane_change_state,
            LaneChangeState::LaneChangeStarting | LaneChangeState::LaneChangeFinishing
        );
        if meta.lane_change_state == LaneChangeState::LaneChangeStarting {
            self.desire_state = *meta
                .desire_state
                .get(if car.left_blinker { 3 } else { 4 })
                .ok_or(Error::Contract("missing lane-change desire"))?;
            self.desire_state_count += 1;
        } else {
            self.desire_state = 0.;
            self.desire_state_count = 0;
        }
        let signal = car.left_blinker != car.right_blinker;
        let direction = if !self.lane_change_active {
            0
        } else if signal {
            if car.left_blinker {
                -1
            } else {
                1
            }
        } else {
            self.lane_change_tracker.direction()
        };
        let valid = input.lane_valid && input.model_ns.abs_diff(input.radar_ns) <= 200_000_000;
        if !valid || direction == 0 {
            self.lane_change_tracker.reset();
            self.lane_change_gap = Plan {
                active: self.lane_change_active,
                reason: if self.lane_change_active {
                    Reason::InvalidInput
                } else {
                    Reason::Inactive
                },
                ..Plan::default()
            };
            self.lane_change_model_ns = 0;
            return Ok(());
        }
        if input.model_ns == self.lane_change_model_ns {
            return Ok(());
        }
        self.lane_change_model_ns = input.model_ns;
        let pose_valid = input.pose_valid && input.model_ns.abs_diff(input.pose_ns) <= 150_000_000;
        self.lane_change_gap = self.lane_change_tracker.update(&lane_change_gap::Input {
            now: timestamp_seconds(input.model_ns)?,
            direction,
            speed: car.v_ego,
            yaw_rate: if pose_valid { input.yaw_rate } else { f64::NAN },
            path_t: input.model.position.t.clone(),
            path_x: input.model.position.x.clone(),
            path_y: input.model.position.y.clone(),
            primary: Some(input.radar.lead_one),
            secondary: Some(input.radar.lead_two),
            blindspot: !signal
                || if direction == -1 {
                    car.left_blindspot
                } else {
                    car.right_blindspot
                },
            valid,
            legacy_side_input: false,
        })?;
        Ok(())
    }
}
