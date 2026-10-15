use super::car_frame::CarFrame;
use super::{
    config::{Config, Mode},
    effects::Effects,
    Controller, Error,
};
use crate::{
    alerts::AlertManager,
    car_specific::CarSpecificEvents,
    cutin::Tracker,
    events::{Catalog, Events},
    helpers::{ExcessiveActuationCheck, PoseCalibrator},
    state::StateMachine,
};
use openpilot_cereal::log_capnp::onroad_event::EventName;
use openpilot_messaging::state::Options;
use std::collections::BTreeSet;
impl Controller {
    pub fn new(
        config: Config,
        localization: (Mode, openpilot_ui_framework::multilang::Multilang),
        effects: &mut impl Effects,
    ) -> Result<Self, Error> {
        let (mode, language) = localization;
        let impact = effects.impact_new()?;
        let excessive_actuation = effects.presence("Offroad_ExcessiveActuation")?;
        effects.setup_publishers()?;
        let gps_service = if effects.boolean("UbloxAvailable")? {
            "gpsLocationExternal"
        } else {
            "gpsLocation"
        }
        .to_owned();
        let disable_dm = 0; // Retired setting has no monitoring authority.
        let use_wide_camera = effects.wide_camera()?;
        let camera_packets: Vec<String> =
            crate::helpers::camera_packets(use_wide_camera, disable_dm, mode.simulation)
                .into_iter()
                .map(str::to_owned)
                .collect();
        let (names, options) = super::subscriptions::specification(
            &camera_packets,
            &gps_service,
            &mode,
            config.not_car,
        );
        effects.setup_subscribers(&names, options)?;
        let is_metric = effects.boolean("IsMetric")?;
        let is_ldw_enabled = effects.boolean("IsLdwEnabled")?;
        if !config.alpha_longitudinal_available {
            effects.remove("AlphaLongitudinalEnabled")?;
        }
        if !config.car.openpilot_longitudinal_control {
            effects.remove("ExperimentalMode")?;
        }
        let personality = effects.personality()?;
        let dm_lockout_set = effects.boolean("DriverTooDistracted")?;
        let ignored_processes = if mode.device_type == "tici" && mode.nvme_present {
            ["loggerd".to_owned()].into()
        } else {
            BTreeSet::new()
        };
        let mut startup_event = if mode.device_type == "mici" {
            None
        } else {
            Some(EventName::Startup)
        };
        if !config.recognized() {
            startup_event = Some(EventName::StartupNoCar);
        } else if config.passive {
            startup_event = Some(EventName::StartupNoControl);
        } else if config.sec_oc_required && !config.sec_oc_key_available {
            startup_event = Some(EventName::StartupNoSecOcKey);
        }
        let mut events = Events::new(Catalog::load(mode.device_type == "mici")?);
        if !config.recognized() {
            events.add(EventName::CarUnrecognized, true);
            effects.offroad("Offroad_CarUnrecognized", None)?;
        } else if config.passive {
            events.add(EventName::DashcamMode, true);
        }
        Ok(Self {
            car_events: CarSpecificEvents::new(config.car.clone()),
            config,
            mode,
            pose_calibrator: PoseCalibrator::default(),
            calibrated_pose: None,
            excessive_actuation_check: ExcessiveActuationCheck::default(),
            excessive_actuation,
            events,
            alerts: AlertManager::default(),
            previous: CarFrame::initial()?,
            impact,
            impact_detector: crate::impact_detector::ImpactDetector::default(),
            radar_warning: crate::radar_only_warning::Warning::default(),
            initialized: false,
            enabled: false,
            active: false,
            mismatch_counter: 0,
            cruise_mismatch_counter: 0,
            last_steering_pressed_frame: 0,
            distance_traveled: 0.0,
            last_functional_fan_frame: 0,
            events_previous: Vec::new(),
            logged_comm_issue: None,
            not_running_previous: None,
            manager_frame: -1,
            not_running_cache: BTreeSet::new(),
            experimental_mode: false,
            personality,
            recalibrating_seen: false,
            dm_lockout_set,
            cutin_tracker: Tracker::default(),
            dm_uncertain_alerted: false,
            dm_disabled_previous: false,
            update_reboot_alerted: false,
            nnff_alerted: false,
            system_ready_alerted: false,
            system_ready_since: None,
            model_startup_complete: false,
            model_starting_current: false,
            car_state_time_s: 0.,
            big_model_loading: false,
            big_model_active: false,
            big_model_ready_time: 0.0,
            state_machine: StateMachine::default(),
            atc_type_previous: String::new(),
            ignored_processes,
            startup_event,
            gps_service,
            camera_packets,
            is_metric,
            is_ldw_enabled,
            disable_dm,
            use_wide_camera,
            language,
            runtime_settings: None,
        })
    }

    pub fn subscription(&self) -> (Vec<&str>, Options) {
        super::subscriptions::specification(
            &self.camera_packets,
            &self.gps_service,
            &self.mode,
            self.config.not_car,
        )
    }
}
