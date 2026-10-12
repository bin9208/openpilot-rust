pub mod gamepad;
mod lane;
mod motion;
mod native_gamepad;
mod random;
use crate::config::{BLUE, BLUE_SOFT};
use crate::models::{ClusterUiState, CruiseDisplayState, LaneMarking, SimulatorInput, TpmsInfo};
pub use random::RandomInputSource;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Direction {
    Left,
    Right,
}
impl Direction {
    const fn sign(self) -> f64 {
        match self {
            Self::Left => -1.0,
            Self::Right => 1.0,
        }
    }
    const fn text(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Phase {
    Idle,
    Changing,
    Recentering,
}
impl Phase {
    const fn text(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Changing => "changing",
            Self::Recentering => "recentering",
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct ClusterSimulator {
    elapsed: f64,
    speed_kph: f64,
    accel_mps2: f64,
    steering: f64,
    left_signal_until: f64,
    right_signal_until: f64,
    lane_change_direction: Option<Direction>,
    lane_change_phase: Phase,
    lane_change_elapsed: f64,
    lane_change_progress: f64,
    lane_change_recenter_start_progress: f64,
    active_lane_position: f64,
    ego_lane_position: f64,
    view_lane_position: f64,
    target_lane_position: f64,
    lane_width_m: f64,
    camera_lane_center_offset_m: Option<f64>,
    steering_angle_deg: Option<f64>,
    surround_yaw_deg: f64,
    surround_pitch_deg: f64,
    surround_view_active: bool,
}
impl Default for ClusterSimulator {
    fn default() -> Self {
        Self {
            elapsed: 0.0,
            speed_kph: 0.0,
            accel_mps2: 0.0,
            steering: 0.0,
            left_signal_until: -999.0,
            right_signal_until: -999.0,
            lane_change_direction: None,
            lane_change_phase: Phase::Idle,
            lane_change_elapsed: 0.0,
            lane_change_progress: 0.0,
            lane_change_recenter_start_progress: 1.0,
            active_lane_position: 0.0,
            ego_lane_position: 0.0,
            view_lane_position: 0.0,
            target_lane_position: 0.0,
            lane_width_m: 3.6,
            camera_lane_center_offset_m: None,
            steering_angle_deg: None,
            surround_yaw_deg: 0.0,
            surround_pitch_deg: 0.0,
            surround_view_active: false,
        }
    }
}
fn clamp(value: f64, low: f64, high: f64) -> f64 {
    let upper = if value < high { value } else { high };
    if upper > low {
        upper
    } else {
        low
    }
}
fn smoothstep(value: f64) -> f64 {
    let value = clamp(value, 0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}
impl ClusterSimulator {
    pub fn update(&mut self, command: &SimulatorInput, dt: f64) -> ClusterUiState {
        let dt = clamp(dt, 0.001, 0.25);
        self.elapsed += dt;
        self.update_steering(command);
        self.update_motion(command, dt);
        if command.left_signal_requested {
            self.start_signal(Direction::Left);
        }
        if command.right_signal_requested {
            self.start_signal(Direction::Right);
        }
        self.update_lane_change(dt);
        self.apply_camera_lane_model(command);
        self.update_surround_view(command, dt);
        self.ui_state(command)
    }
    fn ui_state(&self, command: &SimulatorInput) -> ClusterUiState {
        let limit = if self.speed_kph < 55.0 {
            50
        } else if self.speed_kph < 95.0 {
            80
        } else {
            100
        };
        let highlight = self
            .lane_change_direction
            .filter(|_| self.lane_change_phase == Phase::Changing);
        ClusterUiState {
            speed_kph: self.speed_kph,
            accel_mps2: self.accel_mps2,
            steering: self.steering,
            speed_limit_kph: Some(limit),
            speed_limit_source: Some("sim".into()),
            cruise_kph: Some((limit + 12).min(260)),
            cruise_display_state: CruiseDisplayState::Engaged,
            gear_text: Some("D".into()),
            cruise_gap: Some(3),
            lfa_active: Some(true),
            left_signal: self.elapsed < self.left_signal_until,
            right_signal: self.elapsed < self.right_signal_until,
            left_blindspot: false,
            right_blindspot: false,
            lane_change: self
                .lane_change_direction
                .map(|direction| direction.text().into()),
            lane_change_phase: self.lane_change_phase.text().into(),
            lane_change_progress: self.lane_change_progress,
            highlight_lane: highlight.map(|direction| direction.text().into()),
            highlight_lane_offset: highlight
                .map(|_| self.target_lane_position - self.view_lane_position),
            ego_lane_offset: self.ego_lane_position - self.view_lane_position,
            road_view_lane_position: self.view_lane_position,
            camera_lane_center_offset_m: self.camera_lane_center_offset_m,
            lane_width_m: self.lane_width_m,
            steering_angle_deg: self.steering_angle_deg,
            surround_yaw_deg: self.surround_yaw_deg,
            surround_pitch_deg: self.surround_pitch_deg,
            surround_view_active: self.surround_view_active,
            tpms: TpmsInfo {
                fl: Some(35.0),
                fr: Some(34.0),
                rl: Some(33.0),
                rr: Some(32.0),
            },
            lanes: self.lanes(),
            throttle: command.throttle,
            brake: command.brake,
            ..ClusterUiState::default()
        }
    }
    fn lanes(&self) -> Vec<LaneMarking> {
        let mut lanes = Vec::with_capacity(3);
        if self.lane_change_direction == Some(Direction::Left) {
            lanes.push(LaneMarking {
                offset: self.active_lane_position - 1.5 - self.view_lane_position,
                color: BLUE_SOFT,
                width: 5,
                ..LaneMarking::default()
            });
        }
        lanes.push(LaneMarking {
            offset: self.active_lane_position - 0.5 - self.view_lane_position,
            color: BLUE,
            width: 7,
            ..LaneMarking::default()
        });
        lanes.push(LaneMarking {
            offset: self.active_lane_position + 0.5 - self.view_lane_position,
            color: BLUE,
            width: 7,
            ..LaneMarking::default()
        });
        if self.lane_change_direction == Some(Direction::Right) {
            lanes.push(LaneMarking {
                offset: self.active_lane_position + 1.5 - self.view_lane_position,
                color: BLUE_SOFT,
                style: "dashed".into(),
                width: 5,
                ..LaneMarking::default()
            });
        }
        lanes
    }
}
