use crate::{
    car_state::CarState,
    driving_mode::{DrivingMode, DrivingModeDetector},
    following::{FollowingTime, GapParameters},
    lane_change_gap::{Plan, Tracker},
    model::Model,
    parameters::Parameters,
    radar::Radar,
    traffic_stop::TrafficStopModelLeadMatcher,
    types::{Personality, PlannerMode},
    window::Window,
    Error,
};
use openpilot_cereal::log_capnp::onroad_event::EventName;
use serde::{Deserialize, Serialize};

mod config;
mod desire;
mod navigation;
mod traffic;
mod update;
pub use config::Config;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum XState {
    Lead,
    Cruise,
    E2eCruise,
    E2eStop,
    E2ePrepare,
    E2eStopped,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TrafficState {
    Off,
    Red,
    Green,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Navigation {
    pub atc_type: String,
    pub traffic_state: i32,
    pub active_carrot: i32,
    pub x_dist_to_turn: f64,
    pub desired_speed: f64,
}

pub struct Input<'a> {
    pub car: &'a CarState,
    pub model: &'a Model,
    pub radar: &'a Radar,
    pub personality: Personality,
    pub navigation: Option<&'a Navigation>,
    pub navigation_receive_time: Option<f64>,
    pub mode_checks: bool,
    pub lane_valid: bool,
    pub model_ns: u64,
    pub radar_ns: u64,
    pub pose_ns: u64,
    pub pose_valid: bool,
    pub yaw_rate: f64,
}

#[derive(Debug)]
pub struct CarrotPlanner {
    pub config: Config,
    pub frame: u64,
    params_count: u32,
    pub driving_mode: DrivingMode,
    driving_mode_last: DrivingMode,
    disable_auto: bool,
    detector: DrivingModeDetector,
    pub safe_factor: f64,
    pub follow_factor: f64,
    pub lead_response: i32,
    pub following: FollowingTime,
    pub traffic_state: TrafficState,
    pub x_state: XState,
    stop_filter: Window<3>,
    stop_filter2: Window<15>,
    velocity_filter: Window<10>,
    pub fake_cruise_distance: f64,
    pub x_stop: f64,
    pub actual_stop_distance: f64,
    stopping_count: u32,
    traffic_starting_count: u32,
    pub user_stop_distance: f64,
    start_sign_count: u64,
    stop_sign_count: u64,
    pub comfort_brake: f64,
    pub soft_hold_active: i32,
    pub lane_change_active: bool,
    pub lane_change_gap: Plan,
    lane_change_tracker: Tracker,
    lane_change_model_ns: u64,
    pub desire_state: f64,
    pub desire_state_count: u64,
    traffic_state_carrot: i32,
    carrot_stay_stop: bool,
    pub eco_target_speed: f64,
    pub active_carrot: i32,
    pub distance_to_turn: f64,
    pub atc_type: String,
    pub atc_active: bool,
    stop_x_rate_limited: Option<f64>,
    matcher: TrafficStopModelLeadMatcher,
    pub traffic_stop_model_lead_offset: f64,
    last_event_time: f64,
    pub events: Vec<EventName>,
    pub cruise_speed: f64,
    pub stop_distance: f64,
    pub mode: PlannerMode,
}

impl CarrotPlanner {
    pub fn new(parameters: &mut impl Parameters) -> Result<Self, Error> {
        let driving_mode = DrivingMode::try_from(parameters.integer("MyDrivingMode")?)?;
        let config = Config {
            automatic_mode: parameters.integer("MyDrivingModeAuto")?,
            ..Config::default()
        };
        Ok(Self {
            config,
            frame: 0,
            params_count: 0,
            driving_mode,
            driving_mode_last: driving_mode,
            disable_auto: false,
            detector: DrivingModeDetector::default(),
            safe_factor: 1.,
            follow_factor: 1.,
            lead_response: 0,
            following: FollowingTime::default(),
            traffic_state: TrafficState::Off,
            x_state: XState::Cruise,
            stop_filter: Window::default(),
            stop_filter2: Window::default(),
            velocity_filter: Window::default(),
            fake_cruise_distance: 0.,
            x_stop: 0.,
            actual_stop_distance: 0.,
            stopping_count: 0,
            traffic_starting_count: 0,
            user_stop_distance: -1.,
            start_sign_count: 0,
            stop_sign_count: 0,
            comfort_brake: 2.4,
            soft_hold_active: 0,
            lane_change_active: false,
            lane_change_gap: Plan::default(),
            lane_change_tracker: Tracker::default(),
            lane_change_model_ns: 0,
            desire_state: 0.,
            desire_state_count: 0,
            traffic_state_carrot: 0,
            carrot_stay_stop: false,
            eco_target_speed: 0.,
            active_carrot: 0,
            distance_to_turn: 0.,
            atc_type: String::new(),
            atc_active: false,
            stop_x_rate_limited: None,
            matcher: TrafficStopModelLeadMatcher::default(),
            traffic_stop_model_lead_offset: 0.,
            last_event_time: 0.,
            events: Vec::new(),
            cruise_speed: 0.,
            stop_distance: 0.,
            mode: PlannerMode::Acc,
        })
    }

    fn add_event(&mut self, event: EventName, clock: &mut impl FnMut() -> f64) {
        let now = clock();
        if now - self.last_event_time > 5. {
            let index = self
                .events
                .partition_point(|value| u16::from(*value) <= u16::from(event));
            self.events.insert(index, event);
            self.last_event_time = now;
        }
    }
}

impl<'a> Input<'a> {
    pub fn current_navigation(&self, clock: &mut impl FnMut() -> f64) -> Option<&'a Navigation> {
        let navigation = self.navigation?;
        if let Some(received) = self.navigation_receive_time {
            let age = clock() - received;
            if !(0. ..=1.).contains(&age) {
                return None;
            }
        }
        (0. < navigation.desired_speed && navigation.desired_speed <= 250.).then_some(navigation)
    }
}
