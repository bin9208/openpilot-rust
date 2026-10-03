use super::{
    config::Family, state::State, Error, ALT_GEAR, MEB_GEN2, STOCK_EA_PRESENT, STOCK_HCA_PRESENT,
    STOCK_KLR_PRESENT,
};
use crate::core::Message;
use openpilot_can::Packet;
use openpilot_cereal::car_capnp::{
    car_params::NetworkLocation,
    car_state::{self, button_event::Type as Button},
};
impl State {
    pub(super) fn register(&mut self, now: u64) -> Result<(), Error> {
        match self.config.family {
            Family::Pq => {}
            Family::Mqb => {
                self.pt.add("Blinkmodi_02", Some(1.), false, now)?;
                if self.config.flags & STOCK_HCA_PRESENT != 0 {
                    self.camera.add("HCA_01", Some(1.), false, now)?;
                }
            }
            Family::Meb => {
                let mut pt = vec![
                    ("ESC_51", 100.),
                    ("LH_EPS_03", 100.),
                    ("QFK_01", 100.),
                    ("LWI_01", 100.),
                    ("ESC_50", 50.),
                    ("Motor_51", 50.),
                    ("Motor_14", 10.),
                    ("GRA_ACC_01", 33.),
                    ("Gateway_72", 10.),
                    ("Airbag_02", 5.),
                    ("ESP_21", 50.),
                    ("Blinkmodi_02", 1.),
                    ("SMLS_01", 1.),
                ];
                pt.push(if self.config.flags & ALT_GEAR != 0 {
                    ("Gateway_73", 10.)
                } else {
                    ("Getriebe_11", 50.)
                });
                pt.extend([("VMM_02", 50.), ("ZV_02", 5.)]);
                let gen2 = self.config.flags & MEB_GEN2 != 0;
                if self.config.flags & STOCK_KLR_PRESENT != 0 {
                    pt.push(("KLR_01", if gen2 { 10. } else { 50. }));
                }
                let mut cam = vec![("TA_01", if gen2 { 10. } else { 50. })];
                if self.config.flags & STOCK_EA_PRESENT != 0 {
                    let rate = if gen2 { 2. } else { 10. };
                    cam.extend([("EA_01", rate), ("EA_02", rate)]);
                }
                match self.config.network {
                    NetworkLocation::Gateway => {
                        cam.extend([
                            ("MEB_ACC_01", if gen2 { 10. } else { 50. }),
                            ("ACC_18", 50.),
                        ]);
                        if self.config.bsm {
                            if gen2 {
                                pt.push(("MEB_Side_Assist_01", 10.));
                            } else {
                                cam.push(("MEB_Side_Assist_01", 10.));
                            }
                        }
                    }
                    NetworkLocation::FwdCamera => {
                        pt.push(("MEB_ACC_01", 50.));
                        if self.config.bsm {
                            pt.push(("MEB_Side_Assist_01", 10.));
                        }
                    }
                }
                for (name, rate) in pt {
                    self.pt.add(name, Some(rate), false, now)?;
                }
                for (name, rate) in cam {
                    self.camera.add(name, Some(rate), false, now)?;
                }
            }
        }
        Ok(())
    }
    pub fn update(&mut self, packets: &[Packet], now: u64) -> Result<Message, Error> {
        self.pt.update(packets)?;
        self.camera.update(packets)?;
        self.drain_logs();
        let mut message = Message::new_default();
        let mut ret = message.init_root::<car_state::Builder>();
        match self.config.family {
            Family::Pq => self.pq(&mut ret, now)?,
            Family::Meb => self.meb(&mut ret, now)?,
            Family::Mqb => self.mqb(&mut ret, now)?,
        }
        self.extras.frame = self.extras.frame.checked_add(1).ok_or(Error::Numeric)?;
        ret.set_can_valid(self.pt.can_valid() && self.camera.can_valid());
        ret.set_can_timeout(self.pt.bus_timeout() || self.camera.bus_timeout());
        if ret.reborrow_as_reader().get_v_ego_cluster() == 0. && !self.cluster_seen {
            ret.set_v_ego_cluster(ret.reborrow_as_reader().get_v_ego());
        } else {
            self.cluster_seen = true;
        }
        let speed = ret.reborrow_as_reader().get_cruise_state()?.get_speed();
        ret.reborrow().get_cruise_state()?.set_speed_cluster(speed);
        let mut enabled = false;
        if !self.config.pcm {
            for event in ret.reborrow_as_reader().get_button_events()? {
                enabled |= matches!(event.get_type()?, Button::SetCruise | Button::ResumeCruise)
                    && !event.get_pressed();
            }
        }
        ret.set_button_enable(enabled);
        self.out.set_root(ret.into_reader())?;
        self.drain_logs();
        Ok(message)
    }
}
