use super::{
    can::{self, Acceleration},
    controller::Controller,
    state::State,
    Error, NO_STOP_TIMER, SECOC, UNSUPPORTED_DSU,
};
use openpilot_can::Frame;
use openpilot_cereal::car_capnp::{
    car_control::{self, actuators::LongControlState, h_u_d_control::VisualAlert},
    car_state,
};
use openpilot_control_policy::{
    math::{clip, interp, minimum, sign},
    pid::Step,
};

impl Controller {
    pub(super) fn longitudinal(
        &mut self,
        state: &State,
        cc: car_control::Reader<'_>,
        sends: &mut Vec<Frame>,
    ) -> Result<(), Error> {
        let out = state.out.get_root_as_reader::<car_state::Reader>()?;
        let actuators = cc.get_actuators()?;
        let hud = cc.get_hud_control()?;
        let cancel = cc.get_cruise_control()?.get_cancel();
        let stopping = actuators.get_long_control_state()? == LongControlState::Stopping;
        if out.get_standstill()
            && !self.history.last_standstill
            && self.config.static_flags & NO_STOP_TIMER == 0
        {
            self.history.standstill_req = true;
        }
        if state
            .extras
            .pcm_acc_status
            .ok_or(Error::Stock("pcm_acc_status"))?
            != 8.
        {
            self.history.standstill_req = false;
        }
        self.history.last_standstill = out.get_standstill();
        let fcw = hud.get_visual_alert()? == VisualAlert::Fcw;
        let lead = hud.get_lead_visible() || out.get_v_ego() < 12.;
        if self.config.longitudinal {
            if self.history.frame.is_multiple_of(3) {
                if self.history.frame.is_multiple_of(6) {
                    let desired = 4. - f64::from(hud.get_lead_distance_bars());
                    self.history.distance_button = if out.get_cruise_state()?.get_enabled()
                        && state.extras.pcm_follow_distance != desired
                    {
                        f64::from(self.history.distance_button == 0.)
                    } else {
                        0.
                    };
                }
                let target = f64::from(actuators.get_accel());
                let mut accel = if cc.get_long_active() {
                    clip(
                        target,
                        self.history.prev_accel - 4. * 0.01 * 3.,
                        self.history.prev_accel + 4. * 0.01 * 3.,
                    )
                } else {
                    target
                };
                self.history.prev_accel = accel;
                let pitch = minimum(self.pitch, 0.);
                if pitch.is_infinite() {
                    return Err(Error::Numeric);
                }
                let slope = pitch.sin() * 9.81;
                let net = accel + slope;
                let speed = f64::from(out.get_v_ego());
                let measured = if self.config.flags & SECOC == 0 {
                    interp(
                        speed,
                        &[1., 2.],
                        &[state.extras.gvc, f64::from(out.get_a_ego())],
                    )?
                } else {
                    f64::from(out.get_a_ego())
                };
                let previous = self.aego;
                let dt = 0.01 * 3.;
                let alpha = dt / (0.25 + dt);
                self.aego = (1. - alpha) * self.aego + alpha * measured;
                let jerk = (self.aego - previous) / dt;
                let future_time = interp(speed, &[2., 5.], &[0.25, 0.5])?;
                let future = measured + jerk * future_time;
                if cc.get_long_active() {
                    self.pid.i -= (0.03 * 0.01 * 3.) * sign(self.pid.i);
                    self.pid_speed = speed;
                    accel = self.pid.update(Step {
                        error: accel - future,
                        error_rate: 0.,
                        speed,
                        driver_override: false,
                        feedforward: accel,
                        freeze: actuators.get_long_control_state()? != LongControlState::Pid,
                    })?;
                } else {
                    self.pid.reset();
                }
                let minimum_request = minimum(target + slope, net);
                if minimum_request < 0.2 || stopping || !cc.get_long_active() {
                    self.history.permit_braking = true;
                } else if minimum_request > 0.3 {
                    self.history.permit_braking = false;
                }
                accel = clip(accel, -3.5, self.accel_max);
                sends.push(can::acceleration(
                    &mut self.packer,
                    Acceleration {
                        accel,
                        cancel,
                        braking: self.history.permit_braking,
                        standstill: self.history.standstill_req,
                        lead,
                        acc_type: state.extras.acc_type,
                        fcw,
                        distance: self.history.distance_button,
                    },
                )?);
                self.history.accel = accel;
            }
        } else if cancel {
            if self.config.static_flags & UNSUPPORTED_DSU != 0 {
                sends.push(can::send(
                    &mut self.packer,
                    "PCM_CRUISE",
                    &[
                        ("GAS_RELEASED", 0.),
                        ("CRUISE_ACTIVE", 0.),
                        ("ACC_BRAKING", 0.),
                        ("ACCEL_NET", 0.),
                        ("CRUISE_STATE", 0.),
                        ("CANCEL_REQ", 1.),
                    ],
                )?);
            } else {
                sends.push(can::acceleration(
                    &mut self.packer,
                    Acceleration {
                        accel: 0.,
                        cancel,
                        braking: true,
                        standstill: false,
                        lead,
                        acc_type: state.extras.acc_type,
                        fcw: false,
                        distance: self.history.distance_button,
                    },
                )?);
            }
        }
        Ok(())
    }
}
