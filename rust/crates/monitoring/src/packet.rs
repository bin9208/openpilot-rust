use crate::{AlertLevel, DriverMonitoring, Policy};
use openpilot_cereal::log_capnp::{driver_monitoring_state as wire, event};

fn percent(value: f64) -> Result<i8, capnp::Error> {
    if value.is_nan() {
        return Err(capnp::Error::failed("NaN monitoring percentage".into()));
    }
    // Source int(min(max(v * 100, 0), 100)): truncate only after bounded clamp.
    Ok((value * 100.).clamp(0., 100.) as i8)
}
impl DriverMonitoring {
    /// Serialize all fields through the original cereal schema. The caller owns
    /// validity and monotonic timestamp, as with messaging.new_message.
    pub fn state_packet(&self, valid: bool, log_mono_time: u64) -> Result<Vec<u8>, capnp::Error> {
        let mut message = capnp::message::Builder::new_default();
        let mut event = message.init_root::<event::Builder>();
        event.set_valid(valid);
        event.set_log_mono_time(log_mono_time);
        let mut dm = event.init_driver_monitoring_state();
        dm.set_lockout(self.too_distracted);
        dm.set_lockout_recovery_percent(percent(f64::from(self.lockout_time) / 36000.)?);
        dm.set_alert3_count(
            i8::try_from(self.alert_3_cnt)
                .map_err(|_| capnp::Error::failed("alert3Count exceeds cereal Int8".into()))?,
        );
        dm.set_no_response_count(
            i8::try_from(self.no_response_cnt)
                .map_err(|_| capnp::Error::failed("noResponseCount exceeds cereal Int8".into()))?,
        );
        dm.set_no_response_force_decel(
            self.alert_level == AlertLevel::Three && self.cnt_since_alert_3 >= 100,
        );
        dm.set_alert_count_lockout_percent(percent(f64::from(self.alert_3_cnt) / 2.)?);
        dm.set_alert_time_lockout_percent(percent(f64::from(self.lockout_time) / 36000.)?);
        dm.set_always_on(self.always_on);
        dm.set_always_on_lockout(self.always_on && self.awareness <= self.threshold_alert_2);
        dm.set_alert_level(match self.alert_level {
            AlertLevel::None => wire::AlertLevel::None,
            AlertLevel::One => wire::AlertLevel::One,
            AlertLevel::Two => wire::AlertLevel::Two,
            AlertLevel::Three => wire::AlertLevel::Three,
        });
        dm.set_active_policy(match self.active_policy {
            Policy::Vision => wire::MonitoringPolicy::Vision,
            Policy::Wheeltouch => wire::MonitoringPolicy::Wheeltouch,
        });
        dm.set_is_r_h_d(self.wheel_on_right);
        let mut rhd = dm.reborrow().init_rhd_calibration();
        rhd.set_calibrated_percent(percent(
            f64::from(self.wheelpos_offsetter.filtered_stat.n) / 300.,
        )?);
        // Cereal declares all policy floating point packet fields as Float32.
        rhd.set_offset(self.wheelpos_offsetter.filtered_stat.mean as f32);
        let mut vision = dm.reborrow().init_vision_policy_state();
        vision.set_awareness_percent(percent(if self.active_policy == Policy::Vision {
            self.awareness
        } else {
            self.last_vision_awareness
        })?);
        vision.set_awareness_step(if self.active_policy == Policy::Vision {
            self.step_change as f32
        } else {
            0.
        });
        vision.set_is_distracted(self.driver_distracted);
        let mut types = vision.reborrow().init_distracted_types();
        types.set_pose(self.distracted_types.pose);
        types.set_eye(self.distracted_types.eye);
        types.set_phone(self.distracted_types.phone);
        types.set_sleep(self.distracted_types.sleep);
        vision.set_face_detected(self.face_detected);
        let mut pose = vision.reborrow().init_pose();
        pose.set_pitch(self.pose.pitch as f32);
        pose.set_yaw(self.pose.yaw as f32);
        pose.set_calibrated(self.pose.calibrated);
        pose.set_uncertainty(self.model_std_max as f32);
        let mut pitch = pose.reborrow().init_pitch_calib();
        pitch.set_calibrated_percent(percent(
            f64::from(self.pose.pitch_offsetter.filtered_stat.n) / 1200.,
        )?);
        pitch.set_offset(self.pose.pitch_offsetter.filtered_stat.mean as f32);
        let mut yaw = pose.init_yaw_calib();
        yaw.set_calibrated_percent(percent(
            f64::from(self.pose.yaw_offsetter.filtered_stat.n) / 1200.,
        )?);
        yaw.set_offset(self.pose.yaw_offsetter.filtered_stat.mean as f32);
        vision.set_wheeltouch_fallback_percent(percent(f64::from(self.hi_stds) / 200.)?);
        vision.set_uncertain_offroad_alert_percent(percent(
            f64::from(self.dcam_uncertain_cnt) / 1200.,
        )?);
        let mut wheel = dm.init_wheeltouch_policy_state();
        wheel.set_awareness_percent(percent(if self.active_policy == Policy::Vision {
            self.last_wheeltouch_awareness
        } else {
            self.awareness
        })?);
        wheel.set_awareness_step(if self.active_policy == Policy::Vision {
            0.
        } else {
            self.step_change as f32
        });
        wheel.set_driver_interacting(self.driver_interacting);
        Ok(capnp::serialize::write_message_to_words(&message))
    }
}

impl crate::DriverMonitoring2 {
    pub fn state_packet(
        &self,
        valid: bool,
        now: u64,
        disabled: bool,
        camera_unavailable: bool,
        strict_remaining: f64,
    ) -> Result<Vec<u8>, capnp::Error> {
        let bytes = self.monitor.state_packet(valid, now)?;
        let reader = capnp::serialize::read_message(
            std::io::Cursor::new(bytes),
            capnp::message::ReaderOptions::new(),
        )?;
        let mut message = capnp::message::Builder::new_default();
        message.set_root(reader.get_root::<event::Reader>()?)?;
        let mut root = message.get_root::<event::Builder>()?;
        let event::DriverMonitoringState(dm) = root
            .reborrow()
            .which()
            .map_err(|e| capnp::Error::failed(e.to_string()))?
        else {
            unreachable!("root copied from monitoring state");
        };
        let mut dm = dm?;
        dm.set_camera_unavailable(camera_unavailable);
        dm.set_dm2_disabled(disabled);
        dm.set_dm2_experimental(self.experimental);
        dm.set_dm2_strict_time_remaining(if disabled {
            0.
        } else {
            strict_remaining as f32
        });
        dm.set_dm2_wheel_timeout_factor(if disabled {
            1.
        } else {
            self.wheel_factor as f32
        });
        dm.set_dm2_vision_timeout_factor(if disabled {
            1.
        } else {
            self.vision_factor as f32
        });
        dm.set_dm2_forward_attention_score(if disabled {
            0.
        } else {
            self.forward_score as f32
        });
        dm.set_dm2_forward_recovery(!disabled && self.forward_recovery);
        dm.set_dm2_interaction_credit(if disabled {
            0.
        } else {
            self.input_credit_seconds as f32
        });
        dm.set_dm2_interaction_grace_remaining(if disabled {
            0.
        } else {
            self.interaction_grace_remaining() as f32
        });
        if disabled {
            dm.set_lockout(false);
            dm.set_alert_count_lockout_percent(0);
            dm.set_alert_time_lockout_percent(0);
            dm.set_lockout_recovery_percent(0);
            dm.set_alert3_count(0);
            dm.set_no_response_count(0);
            dm.set_no_response_force_decel(false);
            dm.set_always_on(false);
            dm.set_always_on_lockout(false);
            dm.set_alert_level(wire::AlertLevel::None);
            let mut vision = dm.reborrow().get_vision_policy_state()?;
            vision.set_awareness_percent(100);
            vision.set_awareness_step(0.);
            vision.set_is_distracted(false);
            vision.set_uncertain_offroad_alert_percent(0);
            let mut types = vision.get_distracted_types()?;
            types.set_pose(false);
            types.set_eye(false);
            types.set_phone(false);
            types.set_sleep(false);
            let mut wheel = dm.get_wheeltouch_policy_state()?;
            wheel.set_awareness_percent(100);
            wheel.set_awareness_step(0.);
            wheel.set_driver_interacting(false);
        }
        Ok(capnp::serialize::write_message_to_words(&message))
    }
}
