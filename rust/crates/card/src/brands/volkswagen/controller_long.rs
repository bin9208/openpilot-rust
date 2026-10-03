use super::{can_acc, config::Family, controller::Controller, state::State, Error};
use openpilot_can::Frame;
use openpilot_cereal::car_capnp::{
    car_control::{self, actuators::LongControlState},
    car_state,
};
use openpilot_control_policy::math::clip;
impl Controller {
    pub(super) fn longitudinal(
        &mut self,
        state: &mut State,
        cc: car_control::Reader<'_>,
        sends: &mut Vec<Frame>,
    ) -> Result<(), Error> {
        let act = cc.get_actuators()?;
        state
            .out
            .get_root::<car_state::Builder>()?
            .get_cruise_state()?;
        let out = state.out.get_root_as_reader::<car_state::Reader>()?;
        let available = out.get_cruise_state()?.get_available();
        let fault = out.get_acc_faulted();
        let stopping = act.get_long_control_state()? == LongControlState::Stopping;
        let (enabled, accel, control, hold, starting, override_) = match self.config.family {
            Family::Meb => {
                let accel = if cc.get_enabled() {
                    clip(f64::from(act.get_accel()), -3.5, 2.)
                } else {
                    0.
                };
                let planner = act.get_long_control_state()? == LongControlState::Starting
                    && f64::from(out.get_v_ego()) <= self.config.starting_speed;
                let begin = act.get_long_control_state()? == LongControlState::Pid
                    && state.extras.esp_hold_confirmation
                    && accel >= 0.2
                    && !state.extras.long_control_inhibit;
                if begin {
                    self.history.hold_release_frames = 100;
                } else {
                    self.history.hold_release_frames =
                        self.history.hold_release_frames.saturating_sub(1);
                }
                if !cc.get_enabled() || stopping || accel <= 0. || f64::from(out.get_v_ego()) > 0.3
                {
                    self.history.hold_release_frames = 0;
                }
                let starting = (planner || self.history.hold_release_frames > 0)
                    && !state.extras.long_control_inhibit;
                let override_ = cc.get_cruise_control()?.get_override() || out.get_gas_pressed();
                self.history.long_override_counter = if override_ {
                    self.history.long_override_counter.saturating_add(1).min(5)
                } else {
                    0
                };
                let override_begin = override_ && self.history.long_override_counter < 5;
                let inactive = !cc.get_enabled() || fault;
                self.history.long_disabled_counter = if inactive {
                    self.history.long_disabled_counter.saturating_add(1).min(5)
                } else {
                    0
                };
                let disabling = inactive && self.history.long_disabled_counter < 5;
                let control = if fault {
                    6
                } else if cc.get_enabled() {
                    if override_ {
                        4
                    } else {
                        3
                    }
                } else if available {
                    2
                } else {
                    0
                };
                let releasing = matches!(self.history.acc_hold_type_last, 1 | 4 | 5)
                    && f64::from(out.get_v_ego()) < 5. * (1. / 3.6);
                let hold = if fault || !cc.get_enabled() {
                    if disabling {
                        5
                    } else {
                        0
                    }
                } else if override_ {
                    if override_begin {
                        5
                    } else {
                        0
                    }
                } else if starting {
                    4
                } else if stopping || state.extras.esp_hold_confirmation {
                    1
                } else if releasing {
                    5
                } else {
                    0
                };
                self.history.acc_hold_type_last = hold;
                (cc.get_enabled(), accel, control, hold, starting, override_)
            }
            Family::Mqb | Family::Pq => {
                let control = if matches!(self.config.family, Family::Pq) {
                    if cc.get_long_active() {
                        1
                    } else if available {
                        2
                    } else {
                        0
                    }
                } else if fault {
                    6
                } else if cc.get_long_active() {
                    3
                } else if available {
                    2
                } else {
                    0
                };
                let accel = if cc.get_long_active() {
                    clip(f64::from(act.get_accel()), -3.5, 2.)
                } else {
                    0.
                };
                let starting = act.get_long_control_state()? == LongControlState::Pid
                    && (state.extras.esp_hold_confirmation
                        || f64::from(out.get_v_ego()) < self.config.stopping_speed);
                (cc.get_long_active(), accel, control, 0, starting, false)
            }
        };
        sends.extend(can_acc::acceleration(
            &mut self.packer,
            can_acc::Acc {
                family: self.config.family,
                flags: self.config.flags,
                kind: state.extras.acc_type.ok_or(Error::Stock("acc_type"))?,
                enabled,
                accel,
                control,
                hold,
                stopping,
                starting,
                esp_hold: state.extras.esp_hold_confirmation,
                speed: f64::from(out.get_v_ego_raw()) * 3.6,
                override_,
                travel: state.extras.travel_assist_available,
            },
        )?);
        Ok(())
    }
}
