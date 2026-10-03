use super::{
    can, controller::Controller, controller_cruise::stock, state::State, Error, SEND_INFOTAINMENT,
};
use num_traits::ToPrimitive;
use openpilot_can::Frame;
use openpilot_cereal::car_capnp::car_control::{self, h_u_d_control::VisualAlert};

impl Controller {
    pub(super) fn alerts(
        &mut self,
        state: &State,
        cc: car_control::Reader<'_>,
        sends: &mut Vec<Frame>,
    ) -> Result<(), Error> {
        let hud = cc.get_hud_control()?;
        let alert = hud.get_visual_alert()?;
        let counter = (self.snapshot.frame / 10 % 16)
            .to_f64()
            .ok_or(Error::Numeric)?;
        let mut dash = can::copied(
            stock(&state.extras.es_dashstatus_msg, "es_dashstatus_msg")?,
            can::DASH,
        )?;
        dash.extend([("COUNTER", counter)]);
        if self.longitudinal {
            for (name, value) in [
                ("Cruise_State", 0.),
                ("Cruise_Activated", f64::from(cc.get_enabled())),
                ("Cruise_Disengaged", 0.),
                ("Car_Follow", f64::from(hud.get_lead_visible())),
                ("PCB_Off", 1.),
                ("LDW_Off", 0.),
                ("Cruise_Fault", 0.),
            ] {
                can::set(&mut dash, name, value);
            }
        }
        if [2., 3.].contains(&can::get(&dash, "LKAS_State_Msg")?) {
            can::set(&mut dash, "LKAS_State_Msg", 0.);
        }
        sends.push(can::send(&mut self.packer, "ES_DashStatus", 0, &dash)?);
        let mut lkas = can::copied(
            stock(&state.extras.es_lkas_state_msg, "es_lkas_state_msg")?,
            can::LKAS,
        )?;
        lkas.extend([("COUNTER", counter)]);
        if can::get(&lkas, "LKAS_Alert_Msg")? == 1. {
            can::set(&mut lkas, "LKAS_Alert_Msg", 0.);
        }
        if can::get(&lkas, "LKAS_Alert")? == 27. {
            can::set(&mut lkas, "LKAS_Alert", 0.);
        }
        if can::get(&lkas, "LKAS_Alert")? == 28. && can::get(&lkas, "LKAS_Alert_Msg")? == 7. {
            can::set(&mut lkas, "LKAS_Alert", 0.);
        }
        if can::get(&lkas, "LKAS_Alert")? == 30. {
            can::set(&mut lkas, "LKAS_Alert", 0.);
        }
        if can::get(&lkas, "LKAS_Alert_Msg")? == 7. {
            can::set(&mut lkas, "LKAS_Alert_Msg", 0.);
        }
        if alert == VisualAlert::SteerRequired {
            can::set(&mut lkas, "LKAS_Alert_Msg", 1.);
        }
        if alert == VisualAlert::Ldw && can::get(&lkas, "LKAS_Alert")? == 0. {
            if hud.get_left_lane_depart() {
                can::set(&mut lkas, "LKAS_Alert", 12.);
            } else if hud.get_right_lane_depart() {
                can::set(&mut lkas, "LKAS_Alert", 11.);
            }
        }
        if cc.get_enabled() {
            can::set(&mut lkas, "LKAS_ACTIVE", 1.);
        }
        can::set(
            &mut lkas,
            "LKAS_Dash_State",
            if cc.get_enabled() { 2. } else { 0. },
        );
        can::set(
            &mut lkas,
            "LKAS_Left_Line_Visible",
            f64::from(hud.get_left_lane_visible()),
        );
        can::set(
            &mut lkas,
            "LKAS_Right_Line_Visible",
            f64::from(hud.get_right_lane_visible()),
        );
        sends.push(can::send(&mut self.packer, "ES_LKAS_State", 0, &lkas)?);
        if self.flags & SEND_INFOTAINMENT != 0 {
            let mut info = can::copied(
                stock(&state.extras.es_infotainment_msg, "es_infotainment_msg")?,
                can::INFOTAINMENT,
            )?;
            info.extend([("COUNTER", counter)]);
            if [3., 4.].contains(&can::get(&info, "LKAS_State_Infotainment")?) {
                can::set(&mut info, "LKAS_State_Infotainment", 0.);
            }
            if alert == VisualAlert::SteerRequired {
                can::set(&mut info, "LKAS_State_Infotainment", 3.);
            }
            if alert == VisualAlert::Fcw {
                can::set(&mut info, "LKAS_State_Infotainment", 2.);
            }
            sends.push(can::send(&mut self.packer, "ES_Infotainment", 0, &info)?);
        }
        Ok(())
    }
}
