//! Final owner phases; runtime observation/geometry acquisition stays outside.
use super::{CarrotPlanner, DrivingMode, Input, TrafficState, XState};
use crate::signal::{
    prepare::{self, Geometry},
    priority::Candidate,
    start,
    transport::Context,
};
use openpilot_cereal::log_capnp::onroad_event::EventName;
use serde_json::Value;
pub(super) fn lead_in_stop_range(present: bool, distance: f64, stop: f64) -> bool {
    present && distance.is_finite() && distance > 0. && stop.is_finite() && distance - stop < 2.
}
pub struct Signals<'a> {
    pub suspended: bool,
    pub start: Context,
    pub start_observation: Option<Value>,
    pub prepare: Context,
    pub prepare_observation: Option<Value>,
    pub prepare_actuate: bool,
    pub turning: bool,
    pub geometry: &'a mut dyn Geometry,
}
pub struct StartRead {
    pub context: Context,
    pub observation: Option<Value>,
}
pub struct PrepareRead<'a> {
    pub context: Context,
    pub observation: Option<Value>,
    pub actuate: bool,
    pub turning: bool,
    pub mode_changed: bool,
    pub geometry: &'a mut dyn Geometry,
}
/// Acquisition occurs at the same owner phases as source, after any parameter
/// I/O and immediately before the corresponding state machine is evaluated.
pub trait SignalSource {
    fn policy(&mut self) -> Result<bool, crate::Error>;
    fn start(&mut self) -> Result<StartRead, crate::Error>;
    fn prepare(&mut self) -> Result<PrepareRead<'_>, crate::Error>;
    fn record_start(
        &mut self,
        _decision: &start::Decision,
        _speed: f64,
        _distance: f64,
    ) -> Result<(), crate::Error> {
        Ok(())
    }
    fn record_prepare(&mut self, _decision: &prepare::Decision) -> Result<(), crate::Error> {
        Ok(())
    }
}
impl SignalSource for Signals<'_> {
    fn policy(&mut self) -> Result<bool, crate::Error> {
        Ok(self.suspended)
    }
    fn start(&mut self) -> Result<StartRead, crate::Error> {
        Ok(StartRead {
            context: self.start.clone(),
            observation: self.start_observation.clone(),
        })
    }
    fn prepare(&mut self) -> Result<PrepareRead<'_>, crate::Error> {
        Ok(PrepareRead {
            context: self.prepare.clone(),
            observation: self.prepare_observation.clone(),
            actuate: self.prepare_actuate,
            turning: self.turning,
            mode_changed: false,
            geometry: self.geometry,
        })
    }
}
struct GeometryRef<'a>(&'a mut dyn Geometry);
impl Geometry for GeometryRef<'_> {
    fn range(&mut self, bounds: [f64; 4], night: bool) -> Option<[f64; 2]> {
        self.0.range(bounds, night)
    }
    fn alignment(&mut self, candidate: &Candidate) -> Option<[f64; 2]> {
        self.0.alignment(candidate)
    }
    fn night(&self) -> bool {
        self.0.night()
    }
}
impl CarrotPlanner {
    pub(super) fn signal_policy(&mut self, input: &Input<'_>, suspended: bool) {
        if suspended != self.signal_detection_suspended {
            self.velocity_filter.clear();
            self.stop_sign_count = 0;
            self.start_sign_count = 0;
        }
        self.signal_detection_suspended = suspended;
        if suspended
            && input.car.soft_hold_active == 0
            && self.user_stop_distance < 0.
            && matches!(
                self.x_state,
                XState::E2eStop | XState::E2eStopped | XState::E2ePrepare | XState::E2eStopPrepare
            )
        {
            self.x_state = XState::E2eCruise;
            self.actual_stop_distance = 0.;
            self.stopping_count = 0;
        }
    }
    pub(super) fn signal_start(
        &mut self,
        input: &Input<'_>,
        signals: Option<&StartRead>,
        lead_follow: bool,
    ) -> bool {
        self.signal_observation_color = 0;
        self.signal_green_active = false;
        let Some(s) = signals else {
            self.start_decision = None;
            return false;
        };
        let context = &s.context;
        let request = start::Input {
            now: context.now,
            observation: s.observation.clone(),
            enabled: context.enabled && !self.signal_detection_suspended,
            valid: context.valid,
            drive: context.drive,
            gas: input.car.gas_pressed,
            speed: input.car.v_ego,
            departure: self.traffic_state == TrafficState::Green
                || input.radar.lead_one.status
                || !self.stopping(),
        };
        self.signal_observation_color = start::observation_color(&request);
        let mut decision = self.start_guard.update(&request);
        if lead_follow && decision.hold {
            self.start_guard.clear_unemitted_warning();
            decision = start::Decision {
                reason: "lead_follow".into(),
                ..start::Decision::default()
            };
        }
        if decision.green_confirmed
            && input.car.v_ego < 5. / 3.6
            && matches!(
                self.x_state,
                XState::E2eStop | XState::E2eStopped | XState::E2eCruise
            )
            && self.soft_hold_active == 0
            && !self.carrot_stay_stop
            && self.user_stop_distance < 0.
            && !(input.car.brake_pressed
                || input.car.gas_pressed
                || input.car.left_blinker
                || input.car.right_blinker)
            && self.config.traffic_light_mode == 2
            && self.driving_mode != DrivingMode::High
        {
            self.traffic_state = TrafficState::Green;
            self.signal_green_active = true;
            if decision.released
                && self.x_state == XState::E2eStopped
                && input.car.standstill
                && input.car.v_ego.abs() < 0.1
            {
                self.stopping_count = 0;
            }
        }
        if decision.warn {
            self.insert_signal_event(EventName::AudioRefuse);
        }
        let hold = decision.hold;
        self.start_decision = Some(decision);
        hold
    }
    pub(super) fn signal_prepare(&mut self, input: &Input<'_>, signals: Option<PrepareRead<'_>>) {
        let Some(s) = signals else {
            self.prepare_decision = None;
            self.prepare_control_active = false;
            return;
        };
        if s.mode_changed {
            self.stop_prepare.reset_mode();
        }
        let context = &s.context;
        let request = prepare::Input {
            now: context.now,
            observation: s.observation.clone(),
            enabled: context.enabled
                && !self.signal_detection_suspended
                && self.driving_mode != DrivingMode::High
                && self.config.traffic_light_mode != 0,
            valid: context.valid,
            drive: context.drive,
            speed: input.car.v_ego,
            gas: input.car.gas_pressed,
            brake: input.car.brake_pressed,
            lead: crate::signal::prepare_lead_blocked(input.radar),
            turning: s.turning || input.car.left_blinker || input.car.right_blinker,
            model_stopping: matches!(
                self.x_state,
                XState::E2eStop | XState::E2eStopped | XState::E2ePrepare
            ),
            entry_allowed: self.traffic_starting_count == 0
                && self.user_stop_distance < 0.
                && self.mode == crate::types::PlannerMode::Acc
                && self.soft_hold_active == 0,
        };
        let decision = self
            .stop_prepare
            .update(&request, &mut GeometryRef(s.geometry));
        self.prepare_control_active = decision.active && s.actuate;
        if self.prepare_control_active {
            self.x_state = XState::E2eStopPrepare;
        }
        if decision.entered {
            self.insert_signal_event(EventName::TrafficSignChanged);
        }
        self.prepare_decision = Some(decision);
    }
    pub fn signal_snapshot(&self) -> Value {
        serde_json::json!({"start":self.start_guard.snapshot(),"prepare":self.stop_prepare.snapshot()})
    }
}
