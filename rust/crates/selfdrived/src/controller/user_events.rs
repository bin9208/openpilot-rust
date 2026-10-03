use super::{effects::Effects, views::Views, Controller, Error};
use crate::{
    car_specific::{CarControl, CarInputs, CarState},
    cutin::{self, Candidate},
};
use openpilot_cereal::{
    car_capnp::car_state,
    log_capnp::{self, onroad_event::EventName as E},
};
use openpilot_messaging::state::State;

impl Controller {
    pub fn update_events(
        &mut self,
        cs: car_state::Reader<'_>,
        state: &State,
        lagging: bool,
        effects: &mut impl Effects,
    ) -> Result<(), Error> {
        self.events.clear();
        let views = Views { state };
        if matches!(
            views.controls()?.get_lateral_control_state().which()?,
            log_capnp::controls_state::lateral_control_state::Which::DebugState(_)
        ) {
            self.events.add(E::JoystickDebug, false);
            self.startup_event = None;
        }
        if state.topic("alertDebug")?.receive_frame > 0 {
            self.events.add(E::LongitudinalManeuver, false);
            self.startup_event = None;
        }
        if let Some(event) = self.startup_event.take() {
            self.events.add(event, false);
        }
        if !self.initialized {
            self.events.add(E::SelfdriveInitializing, false);
            return Ok(());
        }
        self.update_reboot_alert(state)?;
        if state.topic("userBookmark")?.updated {
            self.events.add(E::UserBookmark, false);
        }
        if state.topic("audioFeedback")?.updated {
            self.events.add(E::AudioFeedback, false);
        }
        if self.config.passive {
            return Ok(());
        }
        self.cutin_events(state)?;
        let resume = cs
            .get_button_events()?
            .iter()
            .map(|button| button.get_type())
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .any(|kind| {
                matches!(
                    kind,
                    car_state::button_event::Type::AccelCruise
                        | car_state::button_event::Type::ResumeCruise
                )
            });
        if !self.config.car.pcm_cruise && cs.get_v_cruise() > 250.0 && resume {
            self.events.add(E::ResumeBlocked, false);
        }
        self.driver_monitoring(state, effects)?;
        for event in views.plan()?.get_events()?.iter() {
            self.events.add(event.get_name()?, false);
        }
        if cs.get_can_valid() {
            let inputs = CarInputs {
                current: CarState::read(cs)?,
                previous: CarState::read(self.previous.state()?)?,
                control: CarControl::read(views.control()?)?,
            };
            for event in self
                .car_events
                .update(&inputs, effects, self.events.catalog())?
            {
                self.events.add(event, false);
            }
            if self.config.not_car && state.frame() > 200 && self.initialized {
                self.events.add(E::PcmEnable, false);
            }
            let previous = self.previous.state()?;
            if (cs.get_gas_pressed()
                && !previous.get_gas_pressed()
                && effects.boolean("DisengageOnAccelerator")?)
                || (cs.get_brake_pressed()
                    && (!previous.get_brake_pressed() || !cs.get_standstill()))
                || (cs.get_regen_braking()
                    && (!previous.get_regen_braking() || !cs.get_standstill()))
            {
                self.events.add(E::PedalPressed, false);
            }
            if cs.get_lat_enabled() != previous.get_lat_enabled() {
                self.events.add(E::AudioPrompt, false);
            }
        }
        self.hardware_events(state)?;
        self.calibration_events(cs, state, effects)?;
        self.lane_events(cs, state)?;
        self.safety_events(state)?;
        self.health_events(cs, state, lagging, effects)?;
        self.motion_events(cs, state, effects)?;
        Ok(())
    }

    pub fn update_reboot_alert(&mut self, state: &State) -> Result<(), Error> {
        if !self.mode.replay
            && !self.mode.simulation
            && !self.update_reboot_alerted
            && state.frame() as f64 * 0.01 >= 15.0
            && state.all_checks(&["managerState"])?
            && (Views { state }).manager()?.get_reboot_required()
        {
            self.events.add(E::UpdateRebootRequired, false);
            self.update_reboot_alerted = true;
        }
        Ok(())
    }

    fn cutin_events(&mut self, state: &State) -> Result<(), Error> {
        let enabled = self.enabled && state.topic("radarState")?.valid;
        let radar = (Views { state }).radar()?;
        let candidate = |lead: log_capnp::radar_state::lead_data::Reader<'_>| Candidate {
            track_id: lead.get_radar_track_id(),
            d_rel: f64::from(lead.get_d_rel()),
            y_rel: f64::from(lead.get_y_rel()),
            v_rel: f64::from(lead.get_v_rel()),
        };
        let candidates = if enabled {
            radar.get_leads_cut_in()?.iter().map(candidate).collect()
        } else {
            Vec::new()
        };
        let lead_two = radar.get_lead_two()?;
        let promoted = if enabled && lead_two.get_status() {
            Some(candidate(lead_two))
        } else {
            None
        };
        if self
            .cutin_tracker
            .update(&cutin::promoted(&candidates, promoted), enabled)
        {
            self.events.add(E::RadarCutin, false);
        }
        Ok(())
    }

    fn driver_monitoring(
        &mut self,
        state: &State,
        effects: &mut impl Effects,
    ) -> Result<(), Error> {
        use log_capnp::driver_monitoring_state::{AlertLevel, MonitoringPolicy};
        if !self.config.not_car && effects.integer("DisableDM")? == 0 {
            let dm = (Views { state }).dm()?;
            if dm.get_lockout() && !self.dm_lockout_set {
                effects.put_bool("DriverTooDistracted", true)?;
                self.dm_lockout_set = true;
            }
            if dm.get_lockout() || dm.get_always_on_lockout() {
                self.events.add(E::TooDistracted, false);
            }
            let vision = dm.get_active_policy()? == MonitoringPolicy::Vision;
            let event = match dm.get_alert_level()? {
                AlertLevel::One => Some(if vision {
                    E::DriverDistracted1
                } else {
                    E::DriverUnresponsive1
                }),
                AlertLevel::Two => Some(if vision {
                    E::DriverDistracted2
                } else {
                    E::DriverUnresponsive2
                }),
                AlertLevel::Three => Some(if vision {
                    E::DriverDistracted3
                } else {
                    E::DriverUnresponsive3
                }),
                AlertLevel::None => None,
            };
            if let Some(event) = event {
                self.events.add(event, false);
            }
            if dm
                .get_vision_policy_state()?
                .get_uncertain_offroad_alert_percent()
                >= 100
                && !self.dm_uncertain_alerted
            {
                effects.offroad("Offroad_DriverMonitoringUncertain", None)?;
                self.dm_uncertain_alerted = true;
            }
        }
        Ok(())
    }
}
