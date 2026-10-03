use super::{state::State, Error};
use crate::{core::Message, state_helpers};
use openpilot_can::Packet;
use openpilot_cereal::car_capnp::car_state::{self, button_event::Type as Button};

impl State {
    pub fn update(&mut self, packets: &[Packet], now: u64) -> Result<Message, Error> {
        self.pt.update(packets)?;
        self.camera.update(packets)?;
        if let Some(body) = &mut self.body {
            body.update(packets)?;
        }
        self.drain_logs();
        let previous_buttons = self.extras.cruise_buttons;
        let previous_setting = self.extras.cruise_setting;
        self.extras.cruise_setting = self.signal("SCM_BUTTONS", "CRUISE_SETTING", now)?;
        self.extras.cruise_buttons = self.signal("SCM_BUTTONS", "CRUISE_BUTTONS", now)?;
        self.is_metric = self.signal("CAR_SPEED", "IMPERIAL_UNIT", now)? == 0.;
        let mut message = Message::new_default();
        let mut ret = message.init_root::<car_state::Builder>();
        self.motion(&mut ret, now)?;
        self.controls(&mut ret, now)?;
        let mut events = Vec::new();
        for (current, previous, settings) in [
            (self.extras.cruise_buttons, previous_buttons, false),
            (self.extras.cruise_setting, previous_setting, true),
        ] {
            if current != previous {
                for (value, pressed) in [(previous, false), (current, true)] {
                    if value != 0. {
                        let kind = if settings {
                            if value == 3. {
                                Button::GapAdjustCruise
                            } else if value == 1. {
                                Button::Lkas
                            } else {
                                Button::Unknown
                            }
                        } else if value == 4. {
                            Button::AccelCruise
                        } else if value == 3. {
                            Button::DecelCruise
                        } else if value == 1. {
                            Button::MainCruise
                        } else if value == 2. {
                            Button::Cancel
                        } else {
                            Button::Unknown
                        };
                        events.push((kind, pressed));
                    }
                }
            }
        }
        let mut list = ret
            .reborrow()
            .init_button_events(u32::try_from(events.len()).map_err(|_| Error::Numeric)?);
        for (index, (kind, pressed)) in events.into_iter().enumerate() {
            let mut event = list
                .reborrow()
                .get(u32::try_from(index).map_err(|_| Error::Numeric)?);
            event.set_type(kind);
            event.set_pressed(pressed);
        }
        let valid = self.pt.can_valid()
            && self.camera.can_valid()
            && self.body.as_mut().is_none_or(|body| body.can_valid());
        ret.set_can_valid(valid);
        ret.set_can_timeout(
            self.pt.bus_timeout()
                || self.camera.bus_timeout()
                || self.body.as_ref().is_some_and(|b| b.bus_timeout()),
        );
        if ret.reborrow_as_reader().get_v_ego_cluster() == 0. && !self.cluster_seen {
            ret.set_v_ego_cluster(ret.reborrow_as_reader().get_v_ego());
        } else {
            self.cluster_seen = true;
        }
        let speed = ret.reborrow_as_reader().get_cruise_state()?.get_speed();
        ret.reborrow().get_cruise_state()?.set_speed_cluster(speed);
        ret.set_button_enable(state_helpers::button_enable(
            self.config.pcm,
            ret.reborrow_as_reader().get_button_events()?,
        )?);
        self.out.set_root(ret.into_reader())?;
        self.drain_logs();
        Ok(message)
    }
}
