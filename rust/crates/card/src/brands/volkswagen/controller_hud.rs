use super::{can, can_hud, config::Family, controller::Controller, state::State, Error};
use openpilot_can::Frame;
use openpilot_cereal::car_capnp::{
    car_control::{self, h_u_d_control::VisualAlert},
    car_state,
};
impl Controller {
    pub(super) fn hud(
        &mut self,
        state: &State,
        cc: car_control::Reader<'_>,
        sends: &mut Vec<Frame>,
    ) -> Result<(), Error> {
        let hud = cc.get_hud_control()?;
        let out = state.out.get_root_as_reader::<car_state::Reader>()?;
        if self.history.frame.is_multiple_of(self.config.ldw_step()) {
            let alert = if matches!(
                hud.get_visual_alert()?,
                VisualAlert::SteerRequired | VisualAlert::Ldw
            ) {
                if matches!(self.config.family, Family::Pq) {
                    4
                } else {
                    8
                }
            } else {
                0
            };
            sends.push(can::lka(
                &mut self.packer,
                can::Lka {
                    stock: state
                        .extras
                        .ldw_stock_values
                        .as_ref()
                        .ok_or(Error::Stock("ldw_stock_values"))?,
                    active: cc.get_lat_active(),
                    pressed: out.get_steering_pressed(),
                    alert,
                    hud,
                    family: self.config.family,
                },
            )?);
        }
        if self.history.frame.is_multiple_of(self.config.hud_step()) && self.config.longitudinal {
            match self.config.family {
                Family::Meb => {
                    let hud_input = self.meb_hud(state, cc)?;
                    sends.push(can_hud::meb(&mut self.packer, hud_input)?);
                }
                Family::Mqb | Family::Pq => {
                    let lead = if hud.get_lead_visible() && self.history.frame > 100 {
                        if state.extras.upscale_lead_car_signal {
                            512.
                        } else {
                            8.
                        }
                    } else {
                        0.
                    };
                    let status = if out.get_acc_faulted() {
                        6
                    } else if cc.get_long_active() {
                        3
                    } else if out.get_cruise_state()?.get_available() {
                        2
                    } else {
                        0
                    };
                    sends.push(can_hud::legacy(
                        &mut self.packer,
                        can_hud::Legacy {
                            family: self.config.family,
                            status,
                            speed: f64::from(hud.get_set_speed()) * 3.6,
                            lead,
                            bars: f64::from(hud.get_lead_distance_bars()),
                        },
                    )?);
                }
            }
        }
        Ok(())
    }
}
