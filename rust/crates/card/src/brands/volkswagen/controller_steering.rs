use super::{can, config::Family, controller::Controller, state::State, Error, STOCK_HCA_PRESENT};
use num_traits::ToPrimitive;
use openpilot_can::Frame;
use openpilot_cereal::car_capnp::{car_control, car_state};
use openpilot_control_policy::math::{clip, interp, maximum, minimum};
impl Controller {
    pub(super) fn steering(
        &mut self,
        state: &State,
        cc: car_control::Reader<'_>,
    ) -> Result<Vec<Frame>, Error> {
        let mut sends = Vec::new();
        if !self.history.frame.is_multiple_of(2) {
            return Ok(sends);
        }
        let act = cc.get_actuators()?;
        let out = state.out.get_root_as_reader::<car_state::Reader>()?;
        match self.config.family {
            Family::Meb => {
                let (enabled, curvature, power) = if cc.get_lat_active() {
                    let speed = maximum(f64::from(out.get_v_ego_raw()), 1.);
                    let jerk = (5. / speed.powi(2)) * 0.02;
                    let max = (3. + 9.81 * 0.06) / speed.powi(2);
                    let curvature = clip(
                        clip(
                            clip(
                                f64::from(act.get_curvature()),
                                self.history.apply_curvature_last - jerk,
                                self.history.apply_curvature_last + jerk,
                            ),
                            -max,
                            max,
                        ),
                        -0.195,
                        0.195,
                    );
                    let min_power = (self.history.steering_power_last - 2).max(4);
                    let max_power = (self.history.steering_power_last + 2).min(50);
                    let driver = interp(
                        f64::from(out.get_steering_torque().abs()),
                        &[60., 300.],
                        &[50., 4.],
                    )?
                    .to_i32()
                    .ok_or(Error::Numeric)?;
                    let target = interp(
                        f64::from(out.get_v_ego()),
                        &[0., 0.5],
                        &[4., f64::from(driver)],
                    )?
                    .to_i32()
                    .ok_or(Error::Numeric)?;
                    (true, curvature, target.max(min_power).min(max_power))
                } else if self.history.steering_power_last > 0 {
                    (
                        true,
                        clip(state.extras.curvature, -0.195, 0.195),
                        (self.history.steering_power_last - 2).max(0),
                    )
                } else {
                    (false, 0., 0)
                };
                sends.push(can::steering(
                    &mut self.packer,
                    can::Steering {
                        value: curvature,
                        enabled,
                        power,
                        family: self.config.family,
                    },
                )?);
                self.history.apply_curvature_last = curvature;
                self.history.steering_power_last = power;
            }
            Family::Mqb | Family::Pq => {
                let (enabled, torque) = if cc.get_lat_active() {
                    let desired = (f64::from(act.get_torque()) * 300.).round_ties_even();
                    if !desired.is_finite() {
                        return Err(Error::Numeric);
                    }
                    let driver = f64::from(out.get_steering_torque());
                    let high = maximum(minimum(300., 300. + (80. + driver) * 3.), 0.);
                    let low = minimum(maximum(-300., -300. + (-80. + driver) * 3.), 0.);
                    let value = clip(desired, low, high);
                    let up = if matches!(self.config.family, Family::Pq) {
                        6.
                    } else {
                        4.
                    };
                    let previous = f64::from(self.history.apply_torque_last);
                    let limited = if previous > 0. {
                        clip(value, maximum(previous - 10., -up), previous + up)
                    } else {
                        clip(value, previous - up, minimum(previous + 10., up))
                    };
                    let mut torque = limited.round_ties_even().to_i32().ok_or(Error::Numeric)?;
                    self.history.hca_frame_timer_running = self
                        .history
                        .hca_frame_timer_running
                        .checked_add(2)
                        .ok_or(Error::Numeric)?;
                    if self.history.apply_torque_last == torque {
                        self.history.hca_frame_same_torque = self
                            .history
                            .hca_frame_same_torque
                            .checked_add(2)
                            .ok_or(Error::Numeric)?;
                        if self
                            .history
                            .hca_frame_same_torque
                            .to_f64()
                            .ok_or(Error::Numeric)?
                            > 1.9 / 0.01
                        {
                            torque -= if torque < 0 { -1 } else { 1 };
                            self.history.hca_frame_same_torque = 0;
                        }
                    } else {
                        self.history.hca_frame_same_torque = 0;
                    }
                    (torque.abs() > 0, torque)
                } else {
                    (false, 0)
                };
                if !enabled {
                    self.history.hca_frame_timer_running = 0;
                }
                self.history.eps_timer_soft_disable_alert = self
                    .history
                    .hca_frame_timer_running
                    .to_f64()
                    .ok_or(Error::Numeric)?
                    > 350. / 0.01;
                self.history.apply_torque_last = torque;
                sends.push(can::steering(
                    &mut self.packer,
                    can::Steering {
                        value: f64::from(torque),
                        enabled,
                        power: 0,
                        family: self.config.family,
                    },
                )?);
                if self.config.flags & STOCK_HCA_PRESENT != 0 {
                    let mut simulated = clip(f64::from(torque) * 2., -300., 300.);
                    if f64::from(out.get_steering_torque().abs()) > simulated.abs() {
                        simulated = f64::from(out.get_steering_torque());
                    }
                    if matches!(self.config.family, Family::Pq) {
                        return Err(Error::SourceFunction {
                            module: "pqcan",
                            function: "create_eps_update",
                        });
                    }
                    sends.push(can::eps(
                        &mut self.packer,
                        state.extras.eps_stock_values.values()?,
                        simulated,
                    )?);
                }
            }
        }
        Ok(sends)
    }
}
