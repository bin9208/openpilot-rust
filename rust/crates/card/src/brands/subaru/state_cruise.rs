use super::{
    state::{Bus, State},
    state_motion::float,
    Error, GLOBAL_GEN2, HYBRID, PREGLOBAL, SEND_INFOTAINMENT,
};
use openpilot_cereal::car_capnp::car_state;

impl State {
    pub(super) fn cruise(
        &mut self,
        ret: &mut car_state::Builder<'_>,
        now: u64,
    ) -> Result<(), Error> {
        let (bus, msg) = if self.flags & HYBRID != 0 {
            (Bus::Cam, "ES_DashStatus")
        } else {
            (self.chassis_bus(), "CruiseControl")
        };
        let enabled = self.signal(bus, msg, "Cruise_Activated", now)? != 0.;
        let available = self.signal(bus, msg, "Cruise_On", now)? != 0.;
        let mut speed =
            float(self.signal(Bus::Cam, "ES_DashStatus", "Cruise_Set_Speed", now)? * (1. / 3.6))?;
        let units_msg = if self.flags & PREGLOBAL != 0 {
            "Dash_State2"
        } else {
            "Dashlights"
        };
        if self.signal(Bus::Pt, units_msg, "UNITS", now)? == 1. {
            speed = float(f64::from(speed) * 1.609344)?;
        }
        let mut cruise = ret.reborrow().init_cruise_state();
        cruise.set_enabled(enabled);
        cruise.set_available(available);
        cruise.set_speed(speed);
        ret.set_seatbelt_unlatched(self.signal(Bus::Pt, "Dashlights", "SEATBELT_FL", now)? == 1.);
        let mut door = false;
        for signal in [
            "DOOR_OPEN_RR",
            "DOOR_OPEN_RL",
            "DOOR_OPEN_FR",
            "DOOR_OPEN_FL",
        ] {
            door |= self.signal(Bus::Pt, "BodyInfo", signal, now)? != 0.;
        }
        ret.set_door_open(door);
        ret.set_steer_fault_permanent(
            self.signal(Bus::Pt, "Steering_Torque", "Steer_Error_1", now)? == 1.,
        );
        if self.flags & PREGLOBAL != 0 {
            self.extras.cruise_button =
                Some(self.signal(Bus::Cam, "ES_Distance", "Cruise_Button", now)?);
            self.extras.ready =
                Some(self.signal(Bus::Cam, "ES_DashStatus", "Not_Ready_Startup", now)? == 0.);
        } else {
            ret.set_steer_fault_temporary(
                self.signal(Bus::Pt, "Steering_Torque", "Steer_Warning", now)? == 1.,
            );
            ret.reborrow().get_cruise_state()?.set_non_adaptive(
                self.signal(Bus::Cam, "ES_DashStatus", "Conventional_Cruise", now)? == 1.,
            );
            ret.reborrow()
                .get_cruise_state()?
                .set_standstill(self.signal(Bus::Cam, "ES_DashStatus", "Cruise_State", now)? == 3.);
            let alert = self.signal(Bus::Cam, "ES_LKAS_State", "LKAS_Alert", now)?;
            ret.set_stock_fcw(alert == 1. || alert == 2.);
            self.extras.es_lkas_state_msg = Some(self.copied(Bus::Cam, "ES_LKAS_State", now)?);
            let es_bus = if self.flags & GLOBAL_GEN2 != 0 {
                Bus::Alt
            } else {
                Bus::Cam
            };
            self.extras.es_brake_msg = Some(self.copied(es_bus, "ES_Brake", now)?);
            if self.flags & HYBRID == 0 {
                ret.set_stock_aeb(
                    self.signal(self.distance_bus(), "ES_Brake", "AEB_Status", now)? == 8.
                        && self.signal(self.distance_bus(), "ES_Brake", "Brake_Pressure", now)?
                            != 0.,
                );
                self.extras.es_status_msg = Some(self.copied(es_bus, "ES_Status", now)?);
                self.extras.cruise_control_msg =
                    Some(self.copied(self.chassis_bus(), "CruiseControl", now)?);
            }
        }
        if self.flags & HYBRID == 0 {
            self.extras.es_distance_msg =
                Some(self.copied(self.distance_bus(), "ES_Distance", now)?);
        }
        self.extras.es_dashstatus_msg = Some(self.copied(Bus::Cam, "ES_DashStatus", now)?);
        if self.flags & SEND_INFOTAINMENT != 0 {
            self.extras.es_infotainment_msg =
                Some(self.copied(Bus::Cam, "ES_Infotainment", now)?);
        }
        Ok(())
    }
}
