use serde::{Deserialize, Serialize};

pub const DT: f64 = 0.02;
pub const MOVING_SPEED: f64 = 0.10;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StopPhase {
    #[default]
    Idle,
    Approach,
    Request,
    Release,
    Retry,
    Fallback,
    Held,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
pub struct StopInput {
    pub active: bool,
    pub requested: bool,
    pub speed: f64,
    pub held: bool,
    pub accel: f64,
    pub value: f64,
    pub previous_value: f64,
    pub jerk_u: f64,
    pub jerk_l: f64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct StopCommand {
    pub stop_req: u8,
    pub raw: f64,
    pub value: f64,
    pub lower: f64,
}

#[derive(Debug, Serialize)]
pub struct CanfdStopping {
    pub phase: StopPhase,
    pub reason: &'static str,
    pub retried: bool,
    pub elapsed: f64,
    pub no_progress: f64,
    pub distance: f64,
    pub reference_speed: f64,
    pub stopped_time: f64,
    pub rolling_time: f64,
    pub last_value: f64,
}

impl Default for CanfdStopping {
    fn default() -> Self {
        Self {
            phase: StopPhase::Idle,
            reason: "inactive",
            retried: false,
            elapsed: 0.,
            no_progress: 0.,
            distance: 0.,
            reference_speed: 0.,
            stopped_time: 0.,
            rolling_time: 0.,
            last_value: 0.,
        }
    }
}

impl CanfdStopping {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn enter(&mut self, phase: StopPhase, speed: f64, reason: &'static str) {
        self.phase = phase;
        self.reason = reason;
        self.elapsed = 0.;
        self.no_progress = 0.;
        self.distance = 0.;
        self.reference_speed = speed;
    }

    pub fn update(&mut self, input: StopInput) -> Option<StopCommand> {
        let StopInput {
            active,
            requested,
            speed,
            held,
            accel,
            value,
            previous_value,
            jerk_u,
            jerk_l,
        } = input;
        if !active || !requested {
            self.reset();
            return None;
        }
        self.last_value = previous_value;
        if self.phase == StopPhase::Idle {
            self.enter(
                if speed > 0.7 {
                    StopPhase::Approach
                } else {
                    StopPhase::Request
                },
                speed,
                "stop_requested",
            );
        }
        self.elapsed += DT;
        self.distance += speed * DT;
        self.stopped_time = if speed <= 0.05 {
            self.stopped_time + DT
        } else {
            0.
        };
        self.rolling_time = if speed > MOVING_SPEED {
            self.rolling_time + DT
        } else {
            0.
        };
        self.no_progress += DT;
        if speed <= self.reference_speed - 0.03 {
            self.reference_speed = speed;
            self.no_progress = 0.;
        }
        let stopped = (held && speed <= MOVING_SPEED) || self.stopped_time >= 0.2;
        if stopped {
            if self.phase != StopPhase::Held {
                self.enter(StopPhase::Held, speed, "stop_observed");
            }
        } else {
            match self.phase {
                StopPhase::Held => {
                    if self.rolling_time >= 0.2 {
                        self.recover(speed, "motion_after_hold");
                    }
                }
                StopPhase::Approach => {
                    if speed <= 0.7 {
                        self.enter(StopPhase::Request, speed, "entry_speed");
                    }
                }
                StopPhase::Request | StopPhase::Retry => {
                    if speed > 0.7 {
                        self.recover(speed, "speed_above_entry");
                    } else if speed > MOVING_SPEED && self.no_progress >= 0.6 {
                        self.recover(speed, "speed_not_reducing");
                    } else if speed > 0.05 && self.elapsed >= 3. {
                        self.recover(speed, "request_timeout");
                    } else if speed > MOVING_SPEED
                        && self.distance >= 0.5
                        && self.no_progress >= 0.3
                    {
                        self.recover(speed, "creep_distance");
                    }
                }
                StopPhase::Release => {
                    if self.elapsed >= 1. {
                        self.enter(StopPhase::Retry, speed, "reassert_once");
                    }
                }
                StopPhase::Idle | StopPhase::Fallback => {}
            }
        }
        match self.phase {
            StopPhase::Request | StopPhase::Retry | StopPhase::Held => {
                self.last_value = value.min(0.);
                Some(StopCommand {
                    stop_req: 1,
                    raw: accel.min(0.),
                    value: self.last_value,
                    lower: 0.2,
                })
            }
            StopPhase::Idle | StopPhase::Approach | StopPhase::Release | StopPhase::Fallback => {
                let raw = accel.min(-0.5);
                self.last_value = (self.last_value - jerk_l * DT)
                    .max(raw.min(self.last_value + jerk_u * DT))
                    .min(0.);
                Some(StopCommand {
                    stop_req: 0,
                    raw,
                    value: self.last_value,
                    lower: 0.,
                })
            }
        }
    }

    fn recover(&mut self, speed: f64, reason: &'static str) {
        self.enter(
            if self.retried {
                StopPhase::Fallback
            } else {
                StopPhase::Release
            },
            speed,
            reason,
        );
        self.retried = true;
    }
}
