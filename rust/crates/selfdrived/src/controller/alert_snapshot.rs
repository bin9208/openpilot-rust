use super::{views::Views, Controller, Error};
use crate::{
    callbacks::{Health, Process, Snapshot},
    events::Callback,
};
use openpilot_cereal::{car_capnp::car_state, log_capnp};
use openpilot_messaging::state::State;

impl Controller {
    fn alert_snapshot(
        &self,
        callback: &Callback,
        cs: car_state::Reader<'_>,
        state: &State,
    ) -> Result<Snapshot, Error> {
        let views = Views { state };
        let mut snapshot = Snapshot {
            brand: self.config.car.brand.clone().into(),
            flags: self.config.flags,
            min_enable_speed: self.config.car.min_enable_speed,
            min_steer_speed: self.config.car.min_steer_speed,
            ego_speed: f64::from(cs.get_v_ego()),
            ..Snapshot::default()
        };
        match callback {
            Callback::CalibrationIncompleteAlert => {
                let calibration = views.calibration()?;
                snapshot.calibration_recalibrating = calibration.get_cal_status()?
                    == log_capnp::live_calibration_data::Status::Recalibrating;
                snapshot.calibration_percent = calibration.get_cal_perc();
            }
            Callback::CalibrationInvalidAlert => {
                snapshot.calibration_rpy = views
                    .calibration()?
                    .get_rpy_calib()?
                    .iter()
                    .map(f64::from)
                    .collect()
            }
            Callback::AudioFeedbackAlert => {
                snapshot.feedback_block = views.feedback()?.get_block_num()
            }
            Callback::OutOfSpaceAlert => {
                snapshot.free_space_percent = f64::from(views.device()?.get_free_space_percent())
            }
            Callback::PosenetInvalidAlert => {
                snapshot.model_velocity = views
                    .model()?
                    .get_velocity()?
                    .get_x()?
                    .iter()
                    .map(f64::from)
                    .collect()
            }
            Callback::ProcessNotRunningAlert => {
                snapshot.processes = views
                    .manager()?
                    .get_processes()?
                    .iter()
                    .map(|process| {
                        Ok(Process {
                            name: process.get_name()?.to_str()?.to_owned(),
                            running: process.get_running(),
                            should_be_running: process.get_should_be_running(),
                        })
                    })
                    .collect::<Result<_, Error>>()?
            }
            Callback::CommIssueAlert | Callback::CameraMalfunctionAlert => {
                snapshot.health = state
                    .topics()
                    .iter()
                    .map(|topic| {
                        Ok(Health {
                            service: topic.service.name.into(),
                            all_checks: state.all_checks(&[topic.service.name])?,
                        })
                    })
                    .collect::<Result<_, Error>>()?;
            }
            Callback::ParamsdInvalidAlert => {
                let parameters = views.parameters()?;
                snapshot.angle_offset_valid = parameters.get_angle_offset_valid();
                snapshot.angle_offset = f64::from(parameters.get_angle_offset_deg());
                snapshot.steer_ratio_valid = parameters.get_steer_ratio_valid();
                snapshot.steer_ratio = f64::from(parameters.get_steer_ratio());
                snapshot.stiffness_factor_valid = parameters.get_stiffness_factor_valid();
                snapshot.stiffness_factor = f64::from(parameters.get_stiffness_factor());
            }
            Callback::OverheatAlert => {
                let device = views.device()?;
                snapshot.cpu_temps = device.get_cpu_temp_c()?.iter().map(f64::from).collect();
                snapshot.gpu_temps = device.get_gpu_temp_c()?.iter().map(f64::from).collect();
                snapshot.memory_temp = f64::from(device.get_memory_temp_c());
            }
            Callback::LowMemoryAlert => {
                snapshot.memory_usage_percent = views.device()?.get_memory_usage_percent()
            }
            Callback::ModeldLaggingAlert => {
                snapshot.frame_drop_percent = f64::from(views.model()?.get_frame_drop_perc())
            }
            Callback::JoystickAlert => {
                let actuators = views.control()?.get_actuators()?;
                snapshot.accel = f64::from(actuators.get_accel());
                snapshot.torque = f64::from(actuators.get_torque());
            }
            Callback::LongitudinalManeuverAlert => {
                let debug = views.debug()?;
                snapshot.debug_text_1 = debug.get_alert_text1()?.to_str()?.to_owned();
                snapshot.debug_text_2 = debug.get_alert_text2()?.to_str()?.to_owned();
            }
            Callback::BelowEngageSpeedAlert
            | Callback::BelowSteerSpeedAlert
            | Callback::CarParserResult
            | Callback::InvalidLkasSettingAlert
            | Callback::PersonalityChangedAlert
            | Callback::SoftDisableAlert { .. }
            | Callback::StartupMasterAlert
            | Callback::TorqueNnLoadAlert
            | Callback::UserSoftDisableAlert { .. }
            | Callback::WrongCarModeAlert => (),
        }
        Ok(snapshot)
    }

    pub fn update_alerts(
        &mut self,
        cs: car_state::Reader<'_>,
        state: &State,
        effects: &mut impl super::effects::Effects,
    ) -> Result<(), Error> {
        use crate::{callbacks::Context, events::CreateAlertError, state::EventType};
        self.refresh_runtime_settings()?;
        let mut clear = Vec::new();
        if !self
            .state_machine
            .current_alert_types
            .contains(&EventType::Warning)
        {
            clear.push(EventType::Warning);
        }
        if self.enabled {
            clear.push(EventType::NoEntry);
        }
        let personality = self.personality.wire()?;
        let alerts = self
            .events
            .create_alerts(
                &self.state_machine.current_alert_types,
                |callback| {
                    let snapshot = self.alert_snapshot(callback, cs, state)?;
                    Ok::<_, Error>(
                        Context {
                            snapshot: &snapshot,
                            language: &self.language,
                            params: effects,
                            metric: self.is_metric,
                            soft_disable_time: self.state_machine.soft_disable_timer,
                            personality,
                            branch: &self.mode.branch,
                            replay: self.mode.replay,
                            mici: self.mode.device_type == "mici",
                        }
                        .resolve(callback)?,
                    )
                },
                |text| self.language.tr(text).to_owned(),
            )
            .map_err(|error| match error {
                CreateAlertError::UndefinedEvent(event) => Error::UndefinedEvent(event),
                CreateAlertError::Callback(error) => error,
            })?;
        self.alerts.add_many(state.frame(), alerts);
        self.alerts.process_alerts(state.frame(), &clear);
        Ok(())
    }
}
