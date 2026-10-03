use super::{can, controller::Controller, integer, state::State, Error};
use num_traits::ToPrimitive;
use openpilot_can::Frame;
use openpilot_cereal::car_capnp::{car_control, car_state};
impl Controller {
    pub(super) fn steering(
        &mut self,
        state: &State,
        cc: car_control::Reader<'_>,
        now_ns: u64,
        sends: &mut Vec<Frame>,
    ) -> Result<(), Error> {
        let frame = self.snapshot.frame;
        let actuators = cc.get_actuators()?;
        let out = state.out.get_root_as_reader::<car_state::Reader>()?;
        let mut steer_step = if cc.get_lat_active() { 4 } else { 10 };
        if self.config.camera {
            let next = (integer(state.extras.cam_lka_steering_cmd_counter)? + 1).rem_euclid(4);
            let out_of_sync = i32::try_from(self.snapshot.lka_steering_cmd_counter % 4)
                .map_err(|_| Error::Numeric)?
                != next;
            if state.extras.loopback_lka_steering_cmd_ts_nanos == 0 || out_of_sync {
                steer_step = 4;
            }
        }
        self.snapshot.lka_steering_cmd_counter = self
            .snapshot
            .lka_steering_cmd_counter
            .checked_add(u64::from(state.extras.loopback_lka_steering_cmd_updated))
            .ok_or(Error::Numeric)?;
        let elapsed = (i128::from(now_ns)
            - i128::from(state.extras.loopback_lka_steering_cmd_ts_nanos))
        .to_f64()
        .ok_or(Error::Numeric)?
            * 1e-6;
        if frame - self.snapshot.last_steer_frame >= steer_step && elapsed > 15. {
            if state.extras.loopback_lka_steering_cmd_ts_nanos == 0 {
                self.snapshot.lka_steering_cmd_counter =
                    u64::try_from(integer(state.extras.pt_lka_steering_cmd_counter)? + 1)
                        .map_err(|_| Error::Numeric)?;
            }
            let torque = if cc.get_lat_active() {
                self.limits.torque(
                    (f64::from(actuators.get_torque()) * f64::from(self.limits.steer_max))
                        .round_ties_even(),
                    self.snapshot.apply_torque_last,
                    out.get_steering_torque(),
                )?
            } else {
                0
            };
            self.snapshot.last_steer_frame = frame;
            self.snapshot.apply_torque_last = torque;
            sends.push(can::steering(
                &mut self.pt,
                torque,
                i32::try_from(self.snapshot.lka_steering_cmd_counter % 4)
                    .map_err(|_| Error::Numeric)?,
                cc.get_lat_active(),
            )?);
        }
        Ok(())
    }
}
