//! Pinned Hyundai CAN-FD zero-aReq stopping sequence, without release/retry.
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
pub const DT: f64 = 0.02;
pub const MOVING_SPEED: f64 = 0.10;
pub const STOP_LOWER_BAND: f64 = 0.20;
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StopPhase {
    #[default]
    Idle,
    Approach,
    Request,
    Held,
}
impl StopPhase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Approach => "approach",
            Self::Request => "request",
            Self::Held => "held",
        }
    }
}
#[derive(Clone, Copy, Deserialize, Serialize)]
pub struct StopInput {
    pub active: bool,
    pub requested: bool,
    pub speed: f64,
    pub held: bool,
    pub accel: f64,
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
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct CanfdStopping {
    pub last_scc_stop_req: u8,
    pub last_scc_jerk_upper: f64,
    pub phase: StopPhase,
    pub reason: Cow<'static, str>,
    pub stopped_time: f64,
    #[serde(skip)]
    pub warnings: Vec<String>,
}
impl Default for CanfdStopping {
    fn default() -> Self {
        Self {
            last_scc_stop_req: 0,
            last_scc_jerk_upper: 1.,
            phase: StopPhase::Idle,
            reason: Cow::Borrowed("inactive"),
            stopped_time: 0.,
            warnings: Vec::new(),
        }
    }
}
impl CanfdStopping {
    pub fn limit_scc_jerk_upper(&mut self, stop_req: u8, mut jerk_upper: f64) -> f64 {
        if stop_req == 1 && self.last_scc_stop_req != 1 {
            jerk_upper = if self.last_scc_jerk_upper < jerk_upper {
                self.last_scc_jerk_upper
            } else {
                jerk_upper
            };
        }
        self.last_scc_stop_req = stop_req;
        self.last_scc_jerk_upper = jerk_upper;
        jerk_upper
    }
    pub fn reset(&mut self) {
        self.phase = StopPhase::Idle;
        self.reason = Cow::Borrowed("inactive");
        self.stopped_time = 0.;
    }
    pub fn enter(&mut self, phase: StopPhase, reason: impl Into<Cow<'static, str>>) {
        self.phase = phase;
        self.reason = reason.into();
    }
    pub fn update(&mut self, i: StopInput) -> Option<StopCommand> {
        if !i.active || !i.requested {
            self.reset();
            return None;
        }
        if self.phase == StopPhase::Idle {
            self.enter(
                if i.speed > 0.7 {
                    StopPhase::Approach
                } else {
                    StopPhase::Request
                },
                "stop_requested",
            );
        }
        if self.phase == StopPhase::Approach && i.speed <= 0.7 {
            self.enter(StopPhase::Request, "entry_speed");
        }
        self.stopped_time = if i.speed <= 0.05 {
            self.stopped_time + DT
        } else {
            0.
        };
        if (i.held && i.speed <= MOVING_SPEED) || self.stopped_time >= 0.2 {
            if self.phase != StopPhase::Held {
                self.enter(StopPhase::Held, "stop_observed");
            }
        } else if self.phase == StopPhase::Held {
            self.enter(StopPhase::Request, "motion_after_hold");
        }
        if matches!(self.phase, StopPhase::Request | StopPhase::Held) {
            return Some(StopCommand {
                stop_req: 1,
                raw: 0.,
                value: 0.,
                lower: STOP_LOWER_BAND,
            });
        }
        // Preserve Python min/max operand order as well as approach braking.
        let raw = if -0.5 < i.accel { -0.5 } else { i.accel };
        let upper = i.previous_value + i.jerk_u * DT;
        let limited = if upper < raw { upper } else { raw };
        let lower = i.previous_value - i.jerk_l * DT;
        let limited = if limited > lower { limited } else { lower };
        let value = if limited < 0. { limited } else { 0. };
        Some(StopCommand {
            stop_req: 0,
            raw,
            value,
            lower: STOP_LOWER_BAND,
        })
    }
}
