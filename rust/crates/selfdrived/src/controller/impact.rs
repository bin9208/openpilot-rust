use super::{effects::Effects, views::Views, Controller, Error};
use openpilot_cereal::{
    car_capnp::car_state,
    log_capnp::{self, sensor_event_data},
};
use openpilot_messaging::state::State;
impl Controller {
    pub(super) fn update_impact(
        &mut self,
        cs: car_state::Reader<'_>,
        state: &State,
        effects: &mut impl Effects,
    ) -> Result<(), Error> {
        if self.mode.replay || self.mode.simulation || self.config.not_car || self.impact.is_none()
        {
            return Ok(());
        }
        let now = effects.monotonic();
        let (mut trigger, mut quiet, mut acceleration) = (false, None, None);
        let topic = state.topic("accelerometer")?;
        if topic.updated {
            let event = topic.event()?;
            let sample = match event.which()? {
                log_capnp::event::Which::Accelerometer(value) => value?,
                _ => return Err(Error::Contract("accelerometer topic/event union mismatch")),
            };
            let views = Views { state };
            let pose = views.pose()?;
            let calibration = views.calibration()?;
            let orientation = pose.get_orientation_n_e_d()?;
            let valid = topic.valid
                && topic.alive
                && matches!(sample.which()?, sensor_event_data::Which::Acceleration(_))
                && matches!(
                    sample.get_source()?,
                    sensor_event_data::SensorSource::Lsm6ds3
                        | sensor_event_data::SensorSource::Lsm6ds3trc
                )
                && state.all_checks(&["livePose", "liveCalibration"])?
                && orientation.get_valid()
                && pose.get_inputs_o_k()
                && pose.get_sensors_o_k()
                && (0. ..0.2).contains(&(now - pose.get_timestamp() as f64 / 1e9))
                && calibration.get_cal_status()?
                    == log_capnp::live_calibration_data::Status::Calibrated;
            let sensor = match sample.which()? {
                sensor_event_data::Which::Acceleration(value) => {
                    value?.get_v()?.iter().map(f64::from).collect::<Vec<_>>()
                }
                _ => Vec::new(),
            };
            let rpy = calibration
                .get_rpy_calib()?
                .iter()
                .map(f64::from)
                .collect::<Vec<_>>();
            let detection = self.impact_detector.update(
                now,
                sample.get_timestamp() as f64 / 1e9,
                &sensor,
                &[
                    f64::from(orientation.get_x()),
                    f64::from(orientation.get_y()),
                    f64::from(orientation.get_z()),
                ],
                &rpy,
                valid,
            );
            trigger = detection.trigger;
            quiet = detection.quiet;
            acceleration = detection.acceleration;
        } else if !topic.alive {
            self.impact_detector.above = false;
            quiet = Some(false);
        }
        let allowed =
            self.initialized && !self.config.passive && (Views { state }).device()?.get_started();
        effects.impact_update(
            self.impact.as_mut().expect("checked impact adapter"),
            now,
            allowed,
            trigger,
            quiet,
            serde_json::json!({"acceleration":acceleration,"aEgo":cs.get_a_ego()}),
        )
    }
}
