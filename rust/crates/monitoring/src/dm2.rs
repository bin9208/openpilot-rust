//! Final carrot-wip DM2 policy, source e8e152dbcfd9574a1791ed12026d1880e87689b3 (dm2.py).
use crate::{AlertLevel, DT_DMON, DriverMonitoring, Input, InputError, Policy};

pub struct DriverMonitoring2 {
    pub monitor: DriverMonitoring,
    pub experimental: bool,
    pub context_strict: bool,
    pub camera_available: bool,
    pub wheel_factor: f64,
    pub vision_factor: f64,
    pub forward_score: f64,
    pub forward_recovery: bool,
    pub input_credit_seconds: f64,
    pub now: f64,
    forward_frames: u32,
    last_input_event: f64,
    grace_started: f64,
    grace_expired: bool,
    input_received: bool,
    timing_crossed_terminal: bool,
    parked_since: Option<f64>,
    parked_last_check: Option<f64>,
    parked_reset_done: bool,
}
impl DriverMonitoring2 {
    pub fn new(rhd: bool, always_on: bool, experimental: bool) -> Self {
        Self {
            monitor: DriverMonitoring::new(rhd, always_on, false),
            experimental,
            context_strict: false,
            camera_available: true,
            wheel_factor: 1.,
            vision_factor: 1.,
            forward_score: 0.,
            forward_recovery: false,
            input_credit_seconds: 0.,
            now: 0.,
            forward_frames: 0,
            last_input_event: f64::NEG_INFINITY,
            grace_started: f64::NEG_INFINITY,
            grace_expired: true,
            input_received: false,
            timing_crossed_terminal: false,
            parked_since: None,
            parked_last_check: None,
            parked_reset_done: false,
        }
    }
    pub fn experimental_active(&self) -> bool {
        self.experimental && !self.context_strict
    }
    pub fn set_experimental(&mut self, experimental: bool) {
        if self.experimental != experimental {
            self.experimental = experimental;
            self.clear_grace();
        }
    }
    fn clear_grace(&mut self) {
        self.grace_started = f64::NEG_INFINITY;
        self.grace_expired = true;
        self.forward_frames = 0;
        self.forward_recovery = false;
    }
    pub fn configure_context(&mut self, now: f64, camera: bool, strict: bool, clear: bool) {
        self.now = now;
        self.input_credit_seconds = 0.;
        self.input_received = false;
        if strict && !self.context_strict {
            self.clear_grace();
        }
        self.context_strict = strict;
        let experimental = self.experimental_active();
        let empty = experimental && clear;
        let wheel_base = if experimental || !camera {
            [15., 30., 45.]
        } else {
            [5., 15., 25.]
        };
        let wheel_factor = if empty { 2. } else { 1. };
        let vision_factor = if experimental {
            if empty { 4. } else { 2. }
        } else {
            1.
        };
        let mut desired = [
            [5. * vision_factor, 8. * vision_factor, 13. * vision_factor],
            wheel_base.map(|v| v * wheel_factor),
        ];
        let source_changed = camera != self.camera_available;
        let old_kind = usize::from(self.monitor.active_policy == Policy::Wheeltouch);
        let old_budget = self.monitor.timeouts[old_kind][2];
        let changed = source_changed || desired != self.monitor.timeouts;
        self.camera_available = camera;
        self.monitor.relax_pose = camera && experimental;
        self.monitor.experimental_phone = experimental;
        if source_changed {
            self.forward_score = 0.;
            self.forward_frames = 0;
            self.monitor.active_policy = if camera {
                Policy::Vision
            } else {
                Policy::Wheeltouch
            };
        }
        let kind = usize::from(self.monitor.active_policy == Policy::Wheeltouch);
        if !source_changed
            && matches!(
                self.monitor.alert_level,
                AlertLevel::Two | AlertLevel::Three
            )
            && desired[kind][2] > old_budget
        {
            desired[kind] = self.monitor.timeouts[kind];
        }
        self.monitor.timeouts = desired;
        self.wheel_factor = desired[1][2] / wheel_base[2];
        self.vision_factor = desired[0][2] / 13.;
        if now - self.grace_started >= desired[1][2]
            || (source_changed
                && matches!(
                    self.monitor.alert_level,
                    AlertLevel::Two | AlertLevel::Three
                ))
        {
            self.grace_expired = true;
        }
        if changed {
            let [first, second, budget] = desired[kind];
            let previous = self.monitor.awareness;
            if !source_changed && previous > 0. {
                self.monitor.awareness = 1. - (1. - previous) * old_budget / budget;
            }
            self.monitor.threshold_alert_1 = 1. - first / budget;
            self.monitor.threshold_alert_2 = 1. - second / budget;
            self.monitor.step_change = DT_DMON / budget;
            if self.monitor.alert_level == AlertLevel::Two {
                self.monitor.awareness = self.monitor.awareness.min(self.monitor.threshold_alert_2);
            }
            if self.monitor.alert_level == AlertLevel::Three {
                self.monitor.awareness = self.monitor.awareness.min(0.);
            }
            self.timing_crossed_terminal |= previous > 0. && self.monitor.awareness <= 0.;
            self.monitor.last_vision_awareness = self.monitor.awareness;
            self.monitor.last_wheeltouch_awareness = self.monitor.awareness;
        }
    }
    pub fn interaction_grace_remaining(&self) -> f64 {
        if !self.experimental_active()
            || !self.camera_available
            || self.grace_expired
            || self.monitor.alert_level == AlertLevel::Three
            || self.monitor.too_distracted
        {
            0.
        } else {
            (self.monitor.timeouts[1][2] - (self.now - self.grace_started)).max(0.)
        }
    }
    fn response_reset(&mut self) -> bool {
        if self.monitor.alert_level == AlertLevel::Three || self.monitor.too_distracted {
            return false;
        }
        self.monitor.reset_awareness();
        self.monitor.alert_level = AlertLevel::None;
        self.timing_crossed_terminal = false;
        true
    }
    pub fn record_interaction(&mut self, event_time: f64) {
        if !(event_time > self.last_input_event && (0. ..0.25).contains(&(self.now - event_time))) {
            return;
        }
        self.last_input_event = event_time;
        if self.camera_available && !self.experimental_active() {
            return;
        }
        let previous = self.monitor.awareness;
        if self.response_reset() {
            self.input_received = true;
            let kind = usize::from(self.monitor.active_policy == Policy::Wheeltouch);
            self.input_credit_seconds = (1. - previous).max(0.) * self.monitor.timeouts[kind][2];
            if self.experimental_active() {
                self.grace_started = event_time;
                self.grace_expired = false;
            }
        }
    }
    pub fn run_step(&mut self, input: &Input) -> Result<(), InputError> {
        // Base camera state/filter logic is shared; event processing is overridden below.
        let mut normalized = input.clone();
        if input.demo {
            normalized = Input {
                driver: input.driver.clone(),
                car_speed: 30.,
                demo: true,
                ..Input::default()
            };
        }
        let input = &normalized;
        self.monitor.update_camera(input)?;
        let driver = if self.monitor.wheel_on_right {
            &input.driver.right
        } else {
            &input.driver.left
        };
        let pose = &self.monitor.pose;
        let pitch = if pose.calibrated {
            pose.pitch_offsetter
                .filtered_stat
                .mean
                .clamp(-0.0881, 0.124)
        } else {
            0.011
        };
        let yaw = if pose.calibrated {
            pose.yaw_offsetter.filtered_stat.mean.clamp(-0.0246, 0.289)
        } else {
            0.075
        };
        self.forward_score = [
            driver.face_prob,
            driver.left_eye_prob,
            driver.right_eye_prob,
            1. - driver.left_blink_prob,
            1. - driver.right_blink_prob,
            1. - driver.sleep_prob,
            1. - driver.phone_prob,
            1. - driver.sunglasses_prob,
            1. - (pose.pitch - pitch).abs() / 1.5,
            1. - (pose.yaw - yaw).abs() / 1.5,
        ]
        .into_iter()
        .fold(1., f64::min)
        .max(0.);
        let confident = self.experimental_active()
            && self.forward_score >= 0.9
            && pose.low_std
            && !self.monitor.driver_distracted;
        self.forward_frames = if confident {
            (self.forward_frames + 1).min(40)
        } else {
            0
        };
        self.update_events(input);
        Ok(())
    }
    fn update_events(&mut self, input: &Input) {
        self.forward_recovery = false;
        let custom = self.camera_available && self.experimental_active();
        let grace = custom && self.interaction_grace_remaining() > 0.;
        let mut input = input.clone();
        if custom {
            input.steering_pressed = self.input_received;
            input.gas_pressed = false;
            if self.forward_frames >= 40 {
                self.forward_recovery = self.response_reset();
            }
            if grace {
                self.response_reset();
            }
        }
        let previous = self.monitor.alert_3_cnt;
        self.monitor.update_events(&input);
        if grace {
            self.response_reset();
        }
        if self.timing_crossed_terminal
            && self.monitor.alert_level == AlertLevel::Three
            && self.monitor.alert_3_cnt == previous
        {
            self.monitor.alert_3_cnt += 1;
            self.monitor.cnt_since_alert_3 = 0;
        }
        self.timing_crossed_terminal = false;
        if !input.enabled && !self.monitor.always_on {
            self.grace_started = f64::NEG_INFINITY;
        }
    }
    pub fn run_without_camera(
        &mut self,
        response: bool,
        enabled: bool,
        lowspeed: bool,
        wrong_gear: bool,
    ) {
        self.monitor.face_detected = false;
        self.monitor.driver_distracted = false;
        self.monitor.is_model_uncertain = true;
        self.forward_recovery = false;
        self.monitor.set_policy(Policy::Wheeltouch);
        if response {
            self.response_reset();
        }
        self.update_events(&Input {
            steering_pressed: response,
            enabled,
            car_speed: if lowspeed { 0. } else { 3. },
            wrong_gear,
            ..Input::default()
        });
    }
    pub fn update_parked_reset(&mut self, now: f64, eligible: bool) -> bool {
        let continuous = self
            .parked_last_check
            .is_some_and(|last| (0. ..=0.25).contains(&(now - last)));
        self.parked_last_check = Some(now);
        if !eligible || !continuous {
            self.parked_since = None;
            self.parked_reset_done = false;
        }
        if !eligible {
            return false;
        }
        let since = *self.parked_since.get_or_insert(now);
        if self.parked_reset_done || now - since < 1. {
            return false;
        }
        self.parked_reset_done = true;
        self.monitor.too_distracted = false;
        self.monitor.alert_3_cnt = 0;
        self.monitor.cnt_since_alert_3 = 0;
        self.monitor.no_response_cnt = 0;
        self.monitor.lockout_time = 0;
        self.monitor.reset_awareness();
        self.monitor.alert_level = AlertLevel::None;
        self.timing_crossed_terminal = false;
        self.clear_grace();
        self.input_received = false;
        self.input_credit_seconds = 0.;
        true
    }
}
