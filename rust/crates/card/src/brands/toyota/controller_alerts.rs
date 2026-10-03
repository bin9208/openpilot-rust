use super::{can, controller::Controller, state::State, static_dsu, Error, DISABLE_RADAR};
use openpilot_can::Frame;
use openpilot_cereal::car_capnp::car_control::{self, h_u_d_control::VisualAlert};

impl Controller {
    pub(super) fn alerts(
        &mut self,
        state: &State,
        cc: car_control::Reader<'_>,
        sends: &mut Vec<Frame>,
    ) -> Result<(), Error> {
        let frame = self.history.frame;
        if self.config.candidate != "TOYOTA_PRIUS_V" {
            let hud = cc.get_hud_control()?;
            let fcw = hud.get_visual_alert()? == VisualAlert::Fcw;
            let steer = matches!(
                hud.get_visual_alert()?,
                VisualAlert::SteerRequired | VisualAlert::Ldw
            );
            let cancel = cc.get_cruise_control()?.get_cancel();
            let send_ui = if (fcw || steer) != self.history.alert_active {
                self.history.alert_active = !self.history.alert_active;
                true
            } else {
                cancel
            };
            if frame.is_multiple_of(20) || send_ui {
                sends.push(can::ui(
                    &mut self.packer,
                    can::Ui {
                        hud,
                        steer,
                        chime: cancel,
                        enabled: cc.get_enabled(),
                        stock: &state.extras.lkas_hud,
                    },
                )?);
            }
            if (frame.is_multiple_of(100) || send_ui)
                && (self.config.dsu || self.config.flags & DISABLE_RADAR != 0)
            {
                sends.push(can::send(
                    &mut self.packer,
                    "PCS_HUD",
                    &[
                        ("PCS_INDICATOR", 1.),
                        ("FCW", f64::from(fcw)),
                        ("SET_ME_X20", 32.),
                        ("SET_ME_X10", 16.),
                        ("PCS_OFF", 1.),
                        ("PCS_SENSITIVITY", 0.),
                    ],
                )?);
            }
        }
        for message in static_dsu::MESSAGES {
            if frame.is_multiple_of(message.step)
                && self.config.dsu
                && message.candidates.contains(&self.config.candidate.as_str())
            {
                sends.push(Frame {
                    address: message.address,
                    data: message.data.to_vec(),
                    bus: message.bus,
                });
            }
        }
        if frame.is_multiple_of(20) && self.config.flags & DISABLE_RADAR != 0 {
            sends.push(crate::ecu::tester_present(crate::ecu::EcuAddress(
                0x750,
                Some(0xf),
                0,
            )));
        }
        Ok(())
    }
}
