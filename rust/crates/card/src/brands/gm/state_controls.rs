use super::{float, state::State, Error};
use openpilot_cereal::car_capnp::car_state::{self, button_event::Type as Button};
fn button_type(value: f64) -> Button {
    if value == 2. {
        Button::AccelCruise
    } else if value == 3. {
        Button::DecelCruise
    } else if value == 5. {
        Button::MainCruise
    } else if value == 6. {
        Button::Cancel
    } else if value == 7. {
        Button::GapAdjustCruise
    } else {
        Button::Unknown
    }
}
impl State {
    pub(super) fn controls(
        &mut self,
        ret: &mut car_state::Builder<'_>,
        now: u64,
        previous_buttons: f64,
        previous_distance: f64,
    ) -> Result<(), Error> {
        let available = self.signal("ECMEngineStatus", "CruiseMainOn", now)? != 0.;
        self.extras.cruise_main_on = available;
        ret.set_esp_disabled(self.signal("ESPStatus", "TractionControlOn", now)? != 1.);
        let cruise_status = self.signal("AcceleratorPedal2", "CruiseState", now)?;
        let fault = cruise_status == 3.
            || self.signal("EBCMFrictionBrakeStatus", "FrictionBrakeUnavailable", now)? == 1.;
        ret.set_acc_faulted(fault);
        let mut cruise = ret.reborrow().init_cruise_state();
        cruise.set_available(available);
        cruise.set_enabled(cruise_status != 0.);
        cruise.set_standstill(cruise_status == 4. && self.config.model.volt());
        if self.config.camera && self.config.flags & 4 == 0 {
            if !self.config.model.cc {
                cruise.set_speed(float(
                    self.camera_signal("ASCMActiveCruiseControlStatus", "ACCSpeedSetpoint", now)?
                        * (1. / 3.6),
                )?);
            }
            if self.config.pcm && !self.config.model.cc {
                let s =
                    self.camera_signal("ASCMActiveCruiseControlStatus", "ACCCruiseState", now)?;
                cruise.set_non_adaptive(s != 2. && s != 3.);
            }
        }
        if self.config.model.cc {
            cruise.set_speed(float(
                self.signal("ECMCruiseControl", "CruiseSetSpeed", now)? * (1. / 3.6),
            )?);
            cruise.set_enabled(self.signal("ECMCruiseControl", "CruiseActive", now)? != 0.);
            ret.set_acc_faulted(false);
        }
        let previous_lkas = self.extras.lkas_enabled;
        self.extras.lkas_enabled = self.signal("ASCMSteeringButton", "LKAButton", now)?;
        self.extras.pcm_acc_status = Some(cruise_status);
        if self.config.model.cluster_ratio() {
            ret.set_v_clu_ratio(0.96);
        } else if self.config.flags & 16 != 0 {
            self.is_metric = self.settings.get_bool("IsMetric")?;
            let cluster = float(
                self.signal("SPEED_RELATED", "ClusterSpeed", now)?
                    * if self.is_metric {
                        1.609344 * (1. / 3.6)
                    } else {
                        1. / 3.6
                    },
            )?;
            ret.set_v_ego_cluster(cluster);
            self.cluster_speed.update(f64::from(cluster));
            ret.set_v_clu_ratio(if self.config.model.volt() { 1. } else { 0.96 });
        }
        if self.extras.cruise_buttons == 1. && previous_buttons == 0. {
            return Ok(());
        }
        let mut events = Vec::new();
        if self.extras.cruise_buttons != 1. || previous_buttons != 0. {
            for (index, (current, previous, unpressed)) in [
                (self.extras.cruise_buttons, previous_buttons, 1.),
                (self.extras.distance_button, previous_distance, 0.),
                (self.extras.lkas_enabled, previous_lkas, 0.),
            ]
            .into_iter()
            .enumerate()
            {
                if current == previous {
                    continue;
                }
                for (value, pressed) in [(previous, false), (current, true)] {
                    if value == unpressed {
                        continue;
                    }
                    let kind = if index == 0 {
                        button_type(value)
                    } else if value != 1. {
                        Button::Unknown
                    } else if index == 1 {
                        Button::GapAdjustCruise
                    } else {
                        Button::Lkas
                    };
                    events.push((kind, pressed));
                }
            }
        }
        let mut buttons = ret
            .reborrow()
            .init_button_events(u32::try_from(events.len()).map_err(|_| Error::Numeric)?);
        for (index, (kind, pressed)) in events.into_iter().enumerate() {
            let mut b = buttons
                .reborrow()
                .get(u32::try_from(index).map_err(|_| Error::Numeric)?);
            b.set_type(kind);
            b.set_pressed(pressed);
        }
        Ok(())
    }
}
