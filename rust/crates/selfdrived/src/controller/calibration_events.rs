use super::{effects::Effects, views::Views, Controller, Error};
use crate::helpers::{ActuationInput, ExcessiveActuation, Measurement, Pose};
use openpilot_cereal::{
    car_capnp::car_state,
    log_capnp::{self, onroad_event::EventName as E},
};
use openpilot_messaging::state::State;

impl Controller {
    pub(super) fn hardware_events(&mut self, state: &State) -> Result<(), Error> {
        let views = Views { state };
        let device = views.device()?;
        if u16::from(device.get_thermal_status()?)
            >= u16::from(log_capnp::device_state::ThermalStatus::Red)
        {
            self.events.add(E::Overheat, false);
        }
        if device.get_free_space_percent() < 7.0 && !self.mode.simulation {
            self.events.add(E::OutOfSpace, false);
        }
        if device.get_memory_usage_percent() > 90 && !self.mode.simulation {
            self.events.add(E::LowMemory, false);
        }
        let peripheral = views.peripheral()?;
        if peripheral.get_panda_type()? != log_capnp::panda_state::PandaType::Unknown {
            if peripheral.get_fan_speed_rpm() < 500 && device.get_fan_speed_percent_desired() > 50 {
                if (state.frame() - self.last_functional_fan_frame) as f64 * 0.01 > 15.0 {
                    self.events.add(E::FanMalfunction, false);
                }
            } else {
                self.last_functional_fan_frame = state.frame();
            }
        }
        Ok(())
    }

    pub(super) fn calibration_events(
        &mut self,
        cs: car_state::Reader<'_>,
        state: &State,
        effects: &mut impl Effects,
    ) -> Result<(), Error> {
        use log_capnp::live_calibration_data::Status;
        let views = Views { state };
        let calibration = views.calibration()?;
        let status = calibration.get_cal_status()?;
        match status {
            Status::Calibrated => (),
            Status::Uncalibrated => self.events.add(E::CalibrationIncomplete, false),
            Status::Recalibrating => {
                if !self.recalibrating_seen {
                    effects.offroad("Offroad_Recalibration", None)?;
                }
                self.recalibrating_seen = true;
                self.events.add(E::CalibrationRecalibrating, false);
            }
            Status::Invalid => self.events.add(E::CalibrationInvalid, false),
        }
        if self.is_ldw_enabled && state.topic("driverAssistance")?.valid {
            let assistance = views.assistance()?;
            if assistance.get_left_lane_departure() || assistance.get_right_lane_departure() {
                self.events.add(E::Ldw, false);
            }
        }
        if state.topic("liveCalibration")?.updated {
            let rpy = calibration.get_rpy_calib()?;
            if rpy.len() != 3 {
                return Err(Error::Contract("calibration rpy must contain three values"));
            }
            self.pose_calibrator.feed(
                [
                    f64::from(rpy.get(0)),
                    f64::from(rpy.get(1)),
                    f64::from(rpy.get(2)),
                ],
                status == Status::Calibrated,
            );
        }
        if state.topic("livePose")?.updated {
            let pose = views.pose()?;
            let measurement =
                |input: log_capnp::live_pose::x_y_z_measurement::Reader<'_>| Measurement {
                    xyz: [
                        f64::from(input.get_x()),
                        f64::from(input.get_y()),
                        f64::from(input.get_z()),
                    ],
                    xyz_std: [
                        f64::from(input.get_x_std()),
                        f64::from(input.get_y_std()),
                        f64::from(input.get_z_std()),
                    ],
                };
            let pose = Pose {
                orientation: measurement(pose.get_orientation_n_e_d()?),
                velocity: measurement(pose.get_velocity_device()?),
                acceleration: measurement(pose.get_acceleration_device()?),
                angular_velocity: measurement(pose.get_angular_velocity_device()?),
            };
            self.calibrated_pose = Some(self.pose_calibrator.build(&pose));
        }
        if let Some(pose) = &self.calibrated_pose {
            let control = views.control()?;
            let input = ActuationInput {
                longitudinal_active: control.get_long_active(),
                lateral_active: control.get_lat_active(),
                steering_pressed: cs.get_steering_pressed(),
                ego_acceleration: f64::from(cs.get_a_ego()),
                ego_speed: f64::from(cs.get_v_ego()),
                roll: f64::from(views.parameters()?.get_roll()),
            };
            if let Some(excessive) = self.excessive_actuation_check.update(&input, pose)? {
                if !self.excessive_actuation {
                    let text = match excessive {
                        ExcessiveActuation::Longitudinal => "longitudinal",
                        ExcessiveActuation::Lateral => "lateral",
                    };
                    effects.offroad("Offroad_ExcessiveActuation", Some(text))?;
                    self.excessive_actuation = true;
                }
            }
        }
        if self.excessive_actuation {
            self.events.add(E::ExcessiveActuation, false);
        }
        Ok(())
    }

    pub(super) fn lane_events(
        &mut self,
        cs: car_state::Reader<'_>,
        state: &State,
    ) -> Result<(), Error> {
        let views = Views { state };
        if state.topic("carrotMan")?.alive {
            let kind = views.carrot()?.get_atc_type()?.to_str()?;
            if kind != self.atc_type_previous {
                if !kind.contains("prepare") && self.atc_type_previous.contains("prepare") {
                    if kind.contains("fork") {
                        self.events.add(E::AudioLaneChange, false);
                    }
                } else if kind.contains("prepare") {
                } else if kind.contains("turn") && !self.atc_type_previous.contains("turn") {
                    self.events.add(E::AudioTurn, false);
                }
            }
            self.atc_type_previous = kind.to_owned();
        }
        let meta = views.model()?.get_meta()?;
        match meta.get_lane_change_state()? {
            log_capnp::LaneChangeState::PreLaneChange => {
                let direction = meta.get_lane_change_direction()?;
                if (cs.get_left_blindspot() && direction == log_capnp::LaneChangeDirection::Left)
                    || (cs.get_right_blindspot()
                        && direction == log_capnp::LaneChangeDirection::Right)
                {
                    self.events.add(E::LaneChangeBlocked, false);
                } else {
                    self.events.add(
                        if direction == log_capnp::LaneChangeDirection::Left {
                            E::PreLaneChangeLeft
                        } else {
                            E::PreLaneChangeRight
                        },
                        false,
                    );
                }
            }
            log_capnp::LaneChangeState::LaneChangeStarting
            | log_capnp::LaneChangeState::LaneChangeFinishing => {
                self.events.add(E::LaneChange, false)
            }
            log_capnp::LaneChangeState::Off => (),
        }
        Ok(())
    }
}
