mod error;
pub use error::Error;
mod alert_snapshot;
mod calibration_events;
pub mod car_frame;
pub mod config;
mod diagnostics;
pub mod effects;
mod health_events;
mod initialization;
mod motion_events;
pub mod native_effects;
pub mod param_conversion;
mod publication;
mod sampling;
pub mod snapshot;
mod subscriptions;
mod user_events;
pub mod views;

use crate::{
    alerts::AlertManager,
    car_specific::CarSpecificEvents,
    cutin::Tracker,
    events::Events,
    helpers::{ExcessiveActuationCheck, Pose, PoseCalibrator},
    state::StateMachine,
};
use car_frame::CarFrame;
use config::{Config, Mode};
use effects::{Effects, Personality, Publications};
use openpilot_cereal::log_capnp::onroad_event::EventName;
use openpilot_messaging::state::State;
use serde::Serialize;
use std::collections::BTreeSet;

pub const BASE_TOPICS: &[&str] = &[
    "deviceState",
    "pandaStates",
    "peripheralState",
    "modelV2",
    "liveCalibration",
    "carOutput",
    "driverMonitoringState",
    "longitudinalPlan",
    "livePose",
    "managerState",
    "liveParameters",
    "radarState",
    "liveTorqueParameters",
    "carrotMan",
    "controlsState",
    "carControl",
    "driverAssistance",
    "alertDebug",
    "userBookmark",
    "audioFeedback",
];

#[derive(Serialize)]
pub struct Controller {
    pub(crate) config: Config,
    pub(crate) mode: Mode,
    pub(crate) car_events: CarSpecificEvents,
    pub(crate) pose_calibrator: PoseCalibrator,
    pub(crate) calibrated_pose: Option<Pose>,
    pub(crate) excessive_actuation_check: ExcessiveActuationCheck,
    pub(crate) excessive_actuation: bool,
    #[serde(skip)]
    pub(crate) events: Events,
    #[serde(skip)]
    pub(crate) alerts: AlertManager,
    #[serde(skip)]
    pub(crate) previous: CarFrame,
    pub(crate) initialized: bool,
    pub(crate) enabled: bool,
    pub(crate) active: bool,
    pub(crate) mismatch_counter: u64,
    pub(crate) cruise_mismatch_counter: u64,
    pub(crate) last_steering_pressed_frame: i64,
    pub(crate) distance_traveled: f64,
    pub(crate) last_functional_fan_frame: i64,
    #[serde(skip)]
    pub(crate) events_previous: Vec<EventName>,
    pub(crate) logged_comm_issue: Option<diagnostics::Issues>,
    pub(crate) not_running_previous: Option<BTreeSet<String>>,
    pub(crate) experimental_mode: bool,
    pub(crate) personality: Personality,
    pub(crate) recalibrating_seen: bool,
    pub(crate) dm_lockout_set: bool,
    #[serde(skip)]
    pub(crate) cutin_tracker: Tracker,
    pub(crate) dm_uncertain_alerted: bool,
    pub(crate) update_reboot_alerted: bool,
    pub(crate) big_model_loading: bool,
    pub(crate) big_model_active: bool,
    pub(crate) big_model_ready_time: f64,
    pub(crate) state_machine: StateMachine,
    pub(crate) atc_type_previous: String,
    pub(crate) ignored_processes: BTreeSet<String>,
    #[serde(skip)]
    pub(crate) startup_event: Option<EventName>,
    pub(crate) gps_service: String,
    pub(crate) camera_packets: Vec<String>,
    pub(crate) is_metric: bool,
    pub(crate) is_ldw_enabled: bool,
    pub(crate) disable_dm: i32,
    pub(crate) use_wide_camera: bool,
    #[serde(skip)]
    pub(crate) language: openpilot_ui_framework::multilang::Multilang,
    #[serde(skip)]
    pub(crate) runtime_settings: Option<crate::runtime::settings::SharedSettings>,
}

impl Controller {
    pub fn previous_bytes(&self) -> &[u8] {
        &self.previous.bytes
    }
    pub fn params_cycle(&mut self, effects: &mut impl Effects) -> Result<(), Error> {
        self.is_metric = effects.boolean("IsMetric")?;
        self.experimental_mode =
            effects.boolean("ExperimentalMode")? && self.config.car.openpilot_longitudinal_control;
        self.personality = effects.personality()?;
        Ok(())
    }

    pub fn step(
        &mut self,
        input: (&CarFrame, &mut State, bool),
        effects: &mut impl Effects,
        publications: &mut impl Publications,
    ) -> Result<(), Error> {
        let (car, state, lagging) = input;
        let cs = car.state()?;
        self.data_sample(cs, state, effects)?;
        self.update_events(cs, state, lagging, effects)?;
        if !self.config.passive && self.initialized {
            let categories = self
                .events
                .names()
                .iter()
                .flat_map(|event| self.events.categories(*event).map(|item| item.category))
                .collect::<Vec<_>>();
            let flags = self.state_machine.update(&categories);
            self.enabled = flags.enabled;
            self.active = flags.active;
        }
        self.update_alerts(cs, state, effects)?;
        self.publish(state, effects, publications)?;
        self.previous = CarFrame::read(car.bytes.clone())?;
        Ok(())
    }
}
