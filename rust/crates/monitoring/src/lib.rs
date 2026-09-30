//! Port of openpilot/selfdrive/monitoring/policy.py and active scalar filters.
//! This library owns policy and packet construction, not daemon IPC or Params.
mod events;
mod input;
mod packet;
mod pose;
mod scalar;
mod stat;
pub use input::{DriverData, DriverState, Input};
use serde::Serialize;
use stat::RunningStatFilter;

pub const DT_DMON: f64 = 0.05;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum AlertLevel {
    None,
    One,
    Two,
    Three,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Policy {
    Wheeltouch,
    Vision,
}
#[derive(Debug, Default, Serialize)]
pub struct DistractedTypes {
    pub pose: bool,
    pub eye: bool,
    pub phone: bool,
    pub sleep: bool,
}
#[derive(Debug, Serialize)]
pub struct Pose {
    pub yaw: f64,
    pub pitch: f64,
    pub pitch_offsetter: RunningStatFilter,
    pub yaw_offsetter: RunningStatFilter,
    pub calibrated: bool,
    pub low_std: bool,
    pub cfactor_pitch: f64,
    pub cfactor_yaw: f64,
    pub steer_yaw_offset: f64,
}
#[derive(Debug, Default, Serialize)]
pub struct Blink {
    pub left: f64,
    pub right: f64,
}
#[derive(Debug, Serialize)]
pub struct DriverMonitoring {
    pub wheelpos_offsetter: RunningStatFilter,
    pub pose: Pose,
    pub blink: Blink,
    pub phone_prob: f64,
    pub sleep_prob: f64,
    pub alert_level: AlertLevel,
    pub always_on: bool,
    pub distracted_types: DistractedTypes,
    pub driver_distracted: bool,
    pub driver_distraction_filter: f64,
    pub wheel_on_right: bool,
    pub wheel_on_right_last: Option<bool>,
    pub wheel_on_right_default: bool,
    pub face_detected: bool,
    pub alert_3_cnt: u32,
    pub cnt_since_alert_3: u32,
    pub no_response_cnt: u32,
    pub lockout_time: u32,
    pub step_change: f64,
    pub active_policy: Policy,
    pub driver_interacting: bool,
    pub is_model_uncertain: bool,
    pub hi_stds: u32,
    pub model_std_max: f64,
    pub threshold_alert_1: f64,
    pub threshold_alert_2: f64,
    pub dcam_uncertain_cnt: u32,
    pub dcam_reset_cnt: u32,
    pub too_distracted: bool,
    pub awareness: f64,
    pub last_vision_awareness: f64,
    pub last_wheeltouch_awareness: f64,
}
impl DriverMonitoring {
    /// `too_distracted` is the original DriverTooDistracted Params value.
    pub fn new(rhd_saved: bool, always_on: bool, too_distracted: bool) -> Self {
        Self {
            wheelpos_offsetter: RunningStatFilter::new((0.03, 3. * 5.5e-5, 2), None),
            pose: Pose {
                yaw: 0.,
                pitch: 0.,
                pitch_offsetter: RunningStatFilter::new((0.011, 3. * 0.01, 2), Some(7200)),
                yaw_offsetter: RunningStatFilter::new((0.075, 3. * 0.05, 2), Some(7200)),
                calibrated: false,
                low_std: true,
                cfactor_pitch: 1.,
                cfactor_yaw: 1.,
                steer_yaw_offset: 0.,
            },
            blink: Blink::default(),
            phone_prob: 0.,
            sleep_prob: 0.,
            alert_level: AlertLevel::None,
            always_on,
            distracted_types: DistractedTypes::default(),
            driver_distracted: false,
            driver_distraction_filter: 0.,
            wheel_on_right: false,
            wheel_on_right_last: None,
            wheel_on_right_default: rhd_saved,
            face_detected: false,
            alert_3_cnt: 0,
            cnt_since_alert_3: 0,
            no_response_cnt: 0,
            lockout_time: 0,
            step_change: DT_DMON / 13.,
            active_policy: Policy::Vision,
            driver_interacting: false,
            is_model_uncertain: false,
            hi_stds: 0,
            model_std_max: 0.,
            threshold_alert_1: 1. - 5. / 13.,
            threshold_alert_2: 1. - 8. / 13.,
            dcam_uncertain_cnt: 0,
            dcam_reset_cnt: 0,
            too_distracted,
            awareness: 1.,
            last_vision_awareness: 1.,
            last_wheeltouch_awareness: 1.,
        }
    }
    pub fn run_step(&mut self, input: &Input) {
        let demo_input;
        let input = if input.demo {
            demo_input = Input {
                driver: input.driver.clone(),
                car_speed: 30.,
                demo: true,
                ..Input::default()
            };
            &demo_input
        } else {
            input
        };
        let k1 = scalar::max(-0.00156 * (input.car_speed - 16.).powi(2) + 0.6, 0.2);
        let bp = scalar::max(scalar::min(input.brake_disengage_prob / k1, 0.5), 0.);
        self.pose.cfactor_pitch = (0.3237 + bp * ((0.3133 - 0.3237) / 0.5)) / 0.3133;
        self.pose.cfactor_yaw = (0.5042 + bp * ((0.4020 - 0.5042) / 0.5)) / 0.4020;
        self.update_states(input);
        self.update_events(input);
    }
    fn reset_awareness(&mut self) {
        self.awareness = 1.;
        self.last_vision_awareness = 1.;
        self.last_wheeltouch_awareness = 1.;
    }
    fn set_policy(&mut self, target: Policy) {
        if self.active_policy == Policy::Vision && self.awareness <= self.threshold_alert_2 {
            self.step_change = if target == Policy::Vision {
                DT_DMON / 13.
            } else {
                0.
            };
            return;
        } else if self.awareness <= 0. {
            return;
        }
        match target {
            Policy::Vision => {
                if self.active_policy != Policy::Vision {
                    self.last_wheeltouch_awareness = self.awareness;
                    self.awareness = self.last_vision_awareness;
                }
                self.threshold_alert_1 = 1. - 5. / 13.;
                self.threshold_alert_2 = 1. - 8. / 13.;
                self.step_change = DT_DMON / 13.;
            }
            Policy::Wheeltouch => {
                if self.active_policy == Policy::Vision {
                    self.last_vision_awareness = self.awareness;
                    self.awareness = self.last_wheeltouch_awareness;
                }
                self.threshold_alert_1 = 1. - 5. / 25.;
                self.threshold_alert_2 = 1. - 15. / 25.;
                self.step_change = DT_DMON / 25.;
            }
        }
        self.active_policy = target;
    }
}
