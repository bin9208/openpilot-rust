use super::{effects::Effects, views::Views, Controller, Error};
use openpilot_cereal::{
    car_capnp::car_state,
    log_capnp::{self, onroad_event::EventName as E},
};
use openpilot_messaging::state::State;

pub(super) fn lateral_flags(
    controls: log_capnp::controls_state::Reader<'_>,
) -> Result<(bool, bool), Error> {
    use log_capnp::controls_state::lateral_control_state::Which;
    Ok(match controls.get_lateral_control_state().which()? {
        Which::PidState(value) => {
            let value = value?;
            (value.get_active(), value.get_saturated())
        }
        Which::AngleState(value) => {
            let value = value?;
            (value.get_active(), value.get_saturated())
        }
        Which::DebugState(value) => {
            let value = value?;
            (value.get_active(), value.get_saturated())
        }
        Which::TorqueState(value) => {
            let value = value?;
            (value.get_active(), value.get_saturated())
        }
        Which::CurvatureStateDEPRECATED(value) => {
            let value = value?;
            (value.get_active(), value.get_saturated())
        }
        Which::LqrStateDEPRECATED(value) => {
            let value = value?;
            (value.get_active(), value.get_saturated())
        }
        Which::IndiStateDEPRECATED(value) => {
            let value = value?;
            (value.get_active(), value.get_saturated())
        }
    })
}
impl Controller {
    pub(super) fn motion_events(
        &mut self,
        cs: car_state::Reader<'_>,
        state: &State,
        effects: &mut impl Effects,
    ) -> Result<(), Error> {
        let views = Views { state };
        if !self.config.not_car {
            if state.topic("livePose")?.seen && !views.pose()?.get_posenet_o_k() {
                self.events.add(E::PosenetInvalid, false);
            }
            if state.topic("livePose")?.seen && !views.pose()?.get_inputs_o_k() {
                self.events.add(E::LocationdTemporaryError, false);
            }
            if state.topic("liveParameters")?.seen
                && !views.parameters()?.get_valid()
                && views.calibration()?.get_cal_status()?
                    == log_capnp::live_calibration_data::Status::Calibrated
                && !self.mode.testing_closet
                && (!self.mode.simulation || self.mode.replay)
            {
                self.events.add(E::ParamsdTemporaryError, false);
            }
        }
        if ["accelerometer", "gyroscope"].iter().any(|name| {
            state
                .topic(name)
                .is_ok_and(|topic| (state.frame() - topic.receive_frame) as f64 * 0.01 > 10.0)
        }) {
            self.events.add(E::SensorDataInvalid, false);
        }
        if !self.mode.replay {
            self.cruise_mismatch_counter = if cs.get_cruise_state()?.get_enabled() && !self.enabled
            {
                self.cruise_mismatch_counter + 1
            } else {
                0
            };
            if self.cruise_mismatch_counter > 600 {
                self.events.add(E::CruiseMismatch, false);
            }
        }
        if cs.get_steering_pressed() {
            self.last_steering_pressed_frame = state.frame();
        }
        let recent = (state.frame() - self.last_steering_pressed_frame) as f64 * 0.01 < 2.0;
        let controls = views.controls()?;
        let (active, saturated) = lateral_flags(controls)?;
        if active && !recent && !self.config.not_car {
            let speed = if cs.get_v_ego() < 0.3 {
                0.3
            } else {
                f64::from(cs.get_v_ego())
            };
            let actual = f64::from(controls.get_curvature()) * speed.powi(2);
            let desired =
                f64::from(views.model()?.get_action()?.get_desired_curvature()) * speed.powi(2);
            let denominator = (1e-3 + actual).abs();
            if denominator == 0.0 {
                return Err(Error::ZeroDivision);
            }
            let undershooting = desired.abs() / denominator > 1.2;
            let turning = desired.abs() > 1.0;
            if undershooting && turning && saturated {
                self.events.add(E::SteerSaturated, false);
            }
        }
        let stock_braking = self.enabled
            && !self.config.car.openpilot_longitudinal_control
            && cs.get_a_ego() < -1.25;
        let model_fcw = views.model()?.get_meta()?.get_hard_brake_predicted()
            && !cs.get_brake_pressed()
            && !stock_braking;
        let planner_fcw = views.plan()?.get_fcw() && self.enabled;
        if (planner_fcw || model_fcw) && !self.config.not_car {
            self.events.add(E::Fcw, false);
        }
        if !self.mode.simulation || self.mode.replay {
            let gps = state.topic(&self.gps_service)?;
            let gps_ok =
                gps.receive_frame > 0 && (state.frame() - gps.receive_frame) as f64 * 0.01 < 2.0;
            if !gps_ok
                && views.pose()?.get_inputs_o_k()
                && self.distance_traveled > 1500.0
                && self.distance_traveled < 1600.0
            {
                self.events.add(E::NoGps, false);
            }
            self.distance_traveled += f64::from(cs.get_v_ego()).abs() * 0.01;
            if views.model()?.get_frame_drop_perc() > 20.0 {
                self.events.add(E::ModeldLagging, false);
            }
        }
        if state.frame() == 550 && effects.presence("NNFFModelName")? {
            self.events.add(E::TorqueNNLoad, false);
        }
        Ok(())
    }
}
