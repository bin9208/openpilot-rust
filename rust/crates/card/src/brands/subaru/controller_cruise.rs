use super::{
    can,
    controller::Controller,
    state::{State, Values},
    Error, DISABLE_EYESIGHT, GLOBAL_GEN2, HYBRID, PREGLOBAL,
};
use num_traits::ToPrimitive;
use openpilot_can::Frame;
use openpilot_cereal::car_capnp::{car_control, car_state};
use openpilot_control_policy::math::{clip, interp};

pub(super) fn stock<'a>(
    source: &'a Option<Values>,
    name: &'static str,
) -> Result<&'a Values, Error> {
    source.as_ref().ok_or(Error::Stock(name))
}
impl Controller {
    pub(super) fn cruise(
        &mut self,
        state: &State,
        cc: car_control::Reader<'_>,
        sends: &mut Vec<Frame>,
    ) -> Result<(), Error> {
        let accel = f64::from(cc.get_actuators()?.get_accel());
        let (throttle, rpm, brake) = if cc.get_long_active() {
            let throttle = interp(accel, &[0., 2.], &[1818., 3400.])?.round_ties_even();
            let rpm = interp(accel, &[0., 2.], &[600., 3600.])?.round_ties_even();
            let brake = interp(accel, &[-3.5, 0.], &[600., 0.])?.round_ties_even();
            if !throttle.is_finite() || !rpm.is_finite() || !brake.is_finite() {
                return Err(Error::Numeric);
            }
            (
                clip(throttle, 808., 3400.),
                clip(rpm, 0., 3600.),
                clip(brake, 0., 600.),
            )
        } else {
            (1818., 0., 0.)
        };
        let cancel = cc.get_cruise_control()?.get_cancel();
        let frame = self.snapshot.frame;
        if self.flags & PREGLOBAL != 0 {
            if frame.is_multiple_of(5) {
                let available = state
                    .out
                    .get_root_as_reader::<car_state::Reader>()?
                    .get_cruise_state()?
                    .get_available();
                let mut button =
                    if cancel || (!available && state.extras.ready.ok_or(Error::Stock("ready"))?) {
                        1.
                    } else {
                        state
                            .extras
                            .cruise_button
                            .ok_or(Error::Stock("cruise_button"))?
                    };
                if button == 1. && self.snapshot.cruise_button_prev == 1. {
                    button = 0.;
                }
                self.snapshot.cruise_button_prev = button;
                let mut values = can::copied(
                    stock(&state.extras.es_distance_msg, "es_distance_msg")?,
                    can::PREGLOBAL_DISTANCE,
                )?;
                can::set(&mut values, "Cruise_Button", button);
                sends.push(can::preglobal(&mut self.packer, "ES_Distance", values)?);
            }
            return Ok(());
        }
        if frame.is_multiple_of(10) {
            self.alerts(state, cc, sends)?;
        }
        if self.longitudinal {
            if frame.is_multiple_of(5) {
                let counter = (frame / 5 % 16).to_f64().ok_or(Error::Numeric)?;
                let mut values = can::copied(
                    stock(&state.extras.es_status_msg, "es_status_msg")?,
                    can::STATUS,
                )?;
                values.extend([("COUNTER", counter)]);
                for (name, value) in [
                    ("Cruise_RPM", rpm),
                    ("Cruise_Fault", 0.),
                    ("Cruise_Activated", f64::from(cc.get_long_active())),
                ] {
                    can::set(&mut values, name, value);
                }
                sends.push(can::send(&mut self.packer, "ES_Status", 0, &values)?);
                let mut values = can::copied(
                    stock(&state.extras.es_brake_msg, "es_brake_msg")?,
                    can::BRAKE,
                )?;
                values.extend([("COUNTER", counter)]);
                for (name, value) in [
                    ("Cruise_Brake_Fault", 0.),
                    ("Cruise_Activated", f64::from(cc.get_long_active())),
                    ("Brake_Pressure", brake),
                    ("Cruise_Brake_Active", f64::from(brake > 0.)),
                    ("Cruise_Brake_Lights", f64::from(brake >= 70.)),
                ] {
                    can::set(&mut values, name, value);
                }
                sends.push(can::send(&mut self.packer, "ES_Brake", 0, &values)?);
                let mut values = can::copied(
                    stock(&state.extras.es_distance_msg, "es_distance_msg")?,
                    can::DISTANCE,
                )?;
                values.extend([("COUNTER", counter)]);
                for (name, value) in [
                    ("Cruise_Throttle", throttle),
                    ("Cruise_Soft_Disable", 0.),
                    ("Cruise_Fault", 0.),
                    ("Cruise_Brake_Active", f64::from(brake > 0.)),
                ] {
                    can::set(&mut values, name, value);
                }
                if cancel {
                    can::set(&mut values, "Cruise_Cancel", 1.);
                    can::set(&mut values, "Cruise_Throttle", 1818.);
                }
                sends.push(can::send(&mut self.packer, "ES_Distance", 0, &values)?);
            }
        } else if cancel && self.flags & HYBRID == 0 {
            let source = stock(&state.extras.es_distance_msg, "es_distance_msg")?;
            let counter = source
                .get("COUNTER")
                .copied()
                .ok_or_else(|| Error::Signal("COUNTER".into()))?;
            let mut values = can::copied(source, can::DISTANCE)?;
            values.extend([("COUNTER", (counter + 1.) % 16.)]);
            can::set(&mut values, "Cruise_Cancel", 1.);
            can::set(&mut values, "Cruise_Throttle", 1818.);
            sends.push(can::send(
                &mut self.packer,
                "ES_Distance",
                u8::from(self.flags & GLOBAL_GEN2 != 0),
                &values,
            )?);
        }
        if self.flags & DISABLE_EYESIGHT != 0 {
            if frame.is_multiple_of(100) {
                sends.push(Frame {
                    address: 0x787,
                    bus: 2,
                    data: vec![2, 0x3e, 0x80, 0, 0, 0, 0, 0],
                });
            }
            if frame.is_multiple_of(5) {
                sends.push(can::send(
                    &mut self.packer,
                    "ES_HighBeamAssist",
                    0,
                    &[("HBA_Available", 0.)],
                )?);
            }
            if frame.is_multiple_of(10) {
                sends.push(can::send(
                    &mut self.packer,
                    "ES_STATIC_1",
                    0,
                    &[("SET_3", 3.)],
                )?);
            }
            if frame.is_multiple_of(2) {
                sends.push(can::send(
                    &mut self.packer,
                    "ES_STATIC_2",
                    0,
                    &[("SET_3", 3.)],
                )?);
            }
        }
        Ok(())
    }
}
