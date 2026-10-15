use super::{diagnostics, effects::Effects, views::Views, Controller, Error};
use crate::state::EventType;
use openpilot_cereal::{
    car_capnp::{car_params::SafetyModel, car_state},
    log_capnp::{self, onroad_event::EventName as E},
};
use openpilot_logging::{Fields, Value};
use openpilot_messaging::state::State;
use std::collections::BTreeSet;

impl Controller {
    pub(super) fn safety_events(&mut self, state: &State) -> Result<(), Error> {
        for (index, panda) in (Views { state }).pandas()?.iter().enumerate() {
            let mismatch = match self.config.safety.get(index) {
                Some(config) => {
                    panda.get_safety_model()? != config.model
                        || panda.get_safety_param() != config.parameter
                        || panda.get_alternative_experience() != self.config.alternative_experience
                }
                None => {
                    let _ignored = matches!(
                        panda.get_safety_model()?,
                        SafetyModel::Silent | SafetyModel::NoOutput
                    );
                    false
                }
            };
            if (mismatch && state.frame() as f64 * 0.01 > 10.0)
                || panda.get_safety_rx_checks_invalid()
                || self.mismatch_counter >= 200
            {
                self.events.add(E::ControlsMismatch, false);
            }
            for fault in panda.get_faults()?.iter() {
                if fault? == log_capnp::panda_state::FaultType::RelayMalfunction {
                    self.events.add(E::RelayMalfunction, false);
                    break;
                }
            }
        }
        Ok(())
    }

    pub(super) fn health_events(
        &mut self,
        cs: car_state::Reader<'_>,
        state: &State,
        lagging: bool,
        effects: &mut impl Effects,
    ) -> Result<(), Error> {
        let views = Views { state };
        let count = self.events.names().len();
        let external = views.model()?.get_jetlink()?.get_source()?
            == log_capnp::jetlink_frame_status::Source::Jetlink;
        let manager_frame = state.topic("managerState")?.receive_frame;
        if manager_frame != self.manager_frame {
            self.manager_frame = manager_frame;
            let mut refreshed = BTreeSet::new();
            for process in views.manager()?.get_processes()?.iter() {
                if !process.get_running() && process.get_should_be_running() {
                    refreshed.insert(process.get_name()?.to_str()?.to_owned());
                }
            }
            self.not_running_cache = refreshed;
        }
        let mut not_running = self.not_running_cache.clone();
        if !external {
            not_running.remove("jetlinkd");
        }
        if state.topic("managerState")?.receive_frame != 0 && !not_running.is_empty() {
            if self.not_running_previous.as_ref() != Some(&not_running) {
                let mut fields = Fields::new();
                fields.insert(
                    "not_running".into(),
                    diagnostics::process_names(&not_running)?,
                );
                fields.insert("error".into(), Value::Bool(true));
                effects.event("process_not_running", fields)?;
            }
            self.not_running_previous = Some(not_running.clone());
        }
        let dm = (Views { state }).dm()?;
        let fallback = state.all_checks(&["driverMonitoringState"])?
            && (dm.get_camera_unavailable() || dm.get_dm2_disabled());
        let fallback_processes: BTreeSet<String> = if fallback {
            ["dmonitoringmodeld".to_owned()].into()
        } else {
            BTreeSet::new()
        };
        let ignored: BTreeSet<_> = self
            .ignored_processes
            .union(&fallback_processes)
            .cloned()
            .collect();
        if state.topic("managerState")?.receive_frame != 0
            && not_running.difference(&ignored).next().is_some()
        {
            self.events.add(E::ProcessNotRunning, false);
        } else if !self.mode.simulation && !lagging {
            let cameras = self
                .camera_packets
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            if !state.all_alive(&cameras)? {
                self.events.add(E::CameraMalfunction, false);
            } else if !state.all_frequency_ok(&cameras)? {
                self.events.add(E::CameraFrameRate, false);
            }
        }
        if !self.mode.replay && lagging {
            self.events.add(E::SelfdrivedLagging, false);
        }
        let settling = self.big_model_settling(effects)?;
        let pose = views.pose()?;
        let errors = views.radar()?.get_radar_errors()?;
        let inputs_ready = state.all_checks(&[
            "modelV2",
            "radarState",
            "livePose",
            "liveCalibration",
            "liveParameters",
            "longitudinalPlan",
            "driverAssistance",
        ])? && pose.get_inputs_o_k()
            && pose.get_sensors_o_k()
            && pose.get_posenet_o_k();
        if self.enabled || inputs_ready {
            self.model_startup_complete = true;
        }
        let startup_fault = errors.get_can_error()
            || errors.get_radar_fault()
            || errors.get_wrong_config()
            || errors.get_radar_unavailable_temporary()
            || (state.topic("livePose")?.seen
                && (!pose.get_sensors_o_k() || !pose.get_posenet_o_k()));
        self.model_starting_current = settling && !self.model_startup_complete && !startup_fault;
        if self.model_starting_current && !inputs_ready {
            self.events.add(E::SelfdriveInitializing, false);
        }
        if !state.topic("radarState")?.valid {
            let event = if errors.get_can_error() {
                E::CanError
            } else if errors.get_radar_unavailable_temporary() {
                E::RadarTempUnavailable
            } else if errors.get_radar_fault() || errors.get_wrong_config() {
                E::RadarFault
            } else if self.model_starting_current {
                E::SelfdriveInitializing
            } else {
                E::CommIssue
            };
            if event != E::SelfdriveInitializing || !self.events.names().contains(&event) {
                self.events.add(event, false);
            }
        }
        if !state.topic("pandaStates")?.valid {
            self.events.add(E::UsbError, false);
        }
        if cs.get_can_timeout() {
            self.events.add(E::CanBusMissing, false);
        } else if !cs.get_can_valid() {
            self.events.add(E::CanError, false);
        }
        let jetlink = views.model()?.get_jetlink()?;
        if jetlink.get_loss_latched()
            || (jetlink.get_source()? == log_capnp::jetlink_frame_status::Source::Jetlink
                && !state.all_checks(&["modelV2"])?)
        {
            self.events.add(E::JetlinkLost, false);
        }
        let disable = self.events.contains(EventType::NoEntry)
            && (self.events.contains(EventType::SoftDisable)
                || self.events.contains(EventType::ImmediateDisable));
        let no_system_errors = !disable || self.events.names().len() == count;
        if !state.all_checks(&[])? && no_system_errors && !self.model_starting_current {
            self.events.add(
                if !state.all_alive(&[])? {
                    E::CommIssue
                } else if !state.all_frequency_ok(&[])? {
                    E::CommIssueAvgFreq
                } else {
                    E::CommIssue
                },
                false,
            );
            let issues = diagnostics::issues(state);
            if self.logged_comm_issue.as_ref() != Some(&issues) {
                let mut fields = Fields::new();
                fields.insert("error".into(), Value::Bool(true));
                fields.extend(
                    issues
                        .fields()
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone())),
                );
                fields.insert(
                    "timing".into(),
                    diagnostics::communication(state, &issues.services(), effects.monotonic())?,
                );
                effects.event("commIssue", fields)?;
                self.logged_comm_issue = Some(issues);
            }
        } else {
            self.logged_comm_issue = None;
        }
        Ok(())
    }

    pub fn big_model_settling(&mut self, effects: &mut impl Effects) -> Result<bool, Error> {
        let loading = effects.boolean("UsbGpuLoading")?;
        let active = effects.boolean("UsbGpuActive")?;
        if (self.big_model_loading && !loading) || (self.big_model_active && !active) {
            self.big_model_ready_time = effects.monotonic();
        }
        self.big_model_loading = loading;
        self.big_model_active = active;
        Ok(loading || effects.monotonic() < self.big_model_ready_time + 5.0)
    }
}
