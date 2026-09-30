use openpilot_modeld::action::{dynamic_lat_smooth_seconds, Action};

pub struct Settings {
    pub custom_lateral_delay: f64,
    pub lateral_smooth: f64,
    pub longitudinal_delay: f64,
    pub v_ego_stopping: f64,
    pub camera_yaw_trim: f64,
}

pub struct State {
    iteration: u64,
    pub settings: Settings,
    pub lateral_delay: f64,
    pub previous_action: Action,
}

impl State {
    pub fn new(
        longitudinal_actuator_delay: f64,
        v_ego_stopping: f64,
        camera_yaw_trim: f64,
    ) -> Self {
        Self {
            iteration: 0,
            settings: Settings {
                custom_lateral_delay: 0.0,
                lateral_smooth: 0.0,
                longitudinal_delay: longitudinal_actuator_delay + 0.3,
                v_ego_stopping,
                camera_yaw_trim,
            },
            lateral_delay: 0.0,
            previous_action: Action::default(),
        }
    }

    pub fn begin_iteration(&mut self) -> bool {
        self.iteration = self.iteration.wrapping_add(1);
        self.iteration.is_multiple_of(100)
    }

    pub fn refresh(&mut self, settings: Settings) {
        self.settings = settings;
    }

    pub fn action_times(&self) -> (f64, f64) {
        (
            self.lateral_delay + 0.05 + 0.025,
            self.settings.longitudinal_delay + 0.05 + 0.025,
        )
    }

    pub fn complete_action(&mut self, action: Action, live_lateral_delay: f64) {
        self.lateral_delay = if self.settings.custom_lateral_delay > 0.0 {
            self.settings.custom_lateral_delay
        } else {
            live_lateral_delay
        } + dynamic_lat_smooth_seconds(self.settings.lateral_smooth);
        // The source feeds back a cereal Action builder, whose numeric fields are Float32.
        self.previous_action = Action {
            desired_curvature: f64::from(action.desired_curvature as f32),
            desired_acceleration: f64::from(action.desired_acceleration as f32),
            should_stop: action.should_stop,
            desired_velocity: f64::from(action.desired_velocity as f32),
        };
    }
}
