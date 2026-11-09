//! Turn-signal lever rules for DesireHelper (Hyundai CAN-FD lever and stalk counters).
use crate::types::{State, DT};

pub const LEVER_ONE_TOUCH: u8 = 1;
pub const LEVER_LATCHED: u8 = 2;
const TURN_SIGNAL_FLASH_S: f64 = 0.8;
const LEVER_REQUEST_FLASHES: f64 = 5.0;
pub const LEVER_REQUEST_WAIT_S: f64 = LEVER_REQUEST_FLASHES * TURN_SIGNAL_FLASH_S;
const LAMP_DARK_RESET_S: f64 = 1.0;
/// Without a model-detected crossing, a press this early in Finishing is still the
/// driver re-lighting the lamp of the change that just crossed.
pub const STALK_QUEUE_GUARD_S: f64 = 0.3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeverChoice {
    LaneChange,
    Turn,
}

/// (maneuver, undecided) chosen by the lever, or (None, false) when it does not decide.
/// `turn_speed_max` is in m/s, 0 = off. At or below it only the forced-turn gesture
/// is a turn; a lever still in the one-touch detent is undecided, because a latch
/// passes through it for ~0.1 s. Above it every lever input is a lane change.
pub fn lever_maneuver(
    lever: u8,
    v_ego: f64,
    turn_speed_max: f64,
    forced_turn: bool,
) -> (Option<LeverChoice>, bool) {
    if turn_speed_max <= 0. {
        (None, false)
    } else if v_ego > turn_speed_max {
        (Some(LeverChoice::LaneChange), false)
    } else if forced_turn {
        (Some(LeverChoice::Turn), false)
    } else {
        (Some(LeverChoice::LaneChange), lever == LEVER_ONE_TOUCH)
    }
}

/// One-touch lever, lamp lit -> dark -> lit, then latched: the turn gesture.
#[derive(Debug, Default)]
pub struct LeverForcedTurn {
    armed: bool,
    side: u8,
    lightings: u32,
    lit: bool,
    dark_s: f64,
    active: bool,
}

impl LeverForcedTurn {
    pub fn update(&mut self, lever: u8, lamp: u8) -> bool {
        if lever == LEVER_LATCHED {
            self.active = self.active || (self.armed && self.lightings >= 2);
            self.armed = false;
            return self.active;
        }
        self.active = false;
        if lever == LEVER_ONE_TOUCH && !self.armed {
            self.armed = true;
            self.side = lamp;
            self.lightings = u32::from(lamp != 0);
            self.lit = lamp != 0;
            self.dark_s = 0.;
            return false;
        }
        if !self.armed {
            return false;
        }
        if lamp != 0 {
            if lamp != self.side {
                self.side = lamp;
                self.lightings = 0;
            }
            if !self.lit {
                self.lightings += 1;
            }
            self.dark_s = 0.;
        } else {
            self.dark_s += DT;
            if self.dark_s > LAMP_DARK_RESET_S {
                self.armed = false;
            }
        }
        self.lit = lamp != 0;
        false
    }
}

/// A one-touch press asks for one lane change for LEVER_REQUEST_FLASHES flashes.
#[derive(Debug, Default)]
pub struct LeverRequestWait {
    side: u8,
    timer: f64,
    gave_up: bool,
    pub hold_side: u8,
}

impl LeverRequestWait {
    /// Before the FSM (state is last frame's). True while the driver blinker must be ignored.
    pub fn update(
        &mut self,
        enabled: bool,
        stalk_press: u8,
        lever: u8,
        lamp_side: u8,
        state: State,
        start_delay: f64,
    ) -> bool {
        if !enabled {
            *self = Self::default();
            return false;
        }
        if stalk_press != 0 {
            self.side = stalk_press;
            self.timer = 0.;
            self.gave_up = false;
        } else if self.side != 0 {
            self.timer += DT;
        }
        if lever == LEVER_LATCHED || state == State::Starting {
            self.side = 0;
        } else if self.side != 0 && self.timer >= LEVER_REQUEST_WAIT_S + start_delay {
            self.side = 0;
            self.gave_up = true;
        }
        if self.gave_up && lamp_side == 0 {
            self.gave_up = false;
        }
        self.gave_up
    }

    /// After the FSM: the side whose lamp to hold (0 when not waiting for the requested change).
    pub fn update_hold(&mut self, state: State, blinker: u8) -> u8 {
        let waiting = state == State::PreLaneChange && blinker == self.side;
        self.hold_side = if waiting { self.side } else { 0 };
        self.hold_side
    }
}

/// Seconds since the car centre crossed the line toward the change side (-1 before).
#[derive(Debug)]
pub struct LaneCrossing {
    y_min: Option<f64>,
    pub since: f64,
}

impl Default for LaneCrossing {
    fn default() -> Self {
        Self {
            y_min: None,
            since: -1.,
        }
    }
}

impl LaneCrossing {
    const NEAR_M: f64 = 1.0;
    const JUMP_M: f64 = 1.5;

    pub fn update(&mut self, lane_lines: &[Vec<f64>; 4], left: bool) -> f64 {
        if self.since >= 0. {
            self.since += DT;
            return self.since;
        }
        let Some(value) = lane_lines[if left { 1 } else { 2 }].first() else {
            return self.since;
        };
        let y = value.abs();
        let minimum = self.y_min.map_or(y, |previous| previous.min(y));
        self.y_min = Some(minimum);
        if minimum < Self::NEAR_M && y - minimum > Self::JUMP_M {
            self.since = 0.;
        }
        self.since
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_latch_is_a_lane_change_and_gesture_is_a_turn() {
        assert_eq!(lever_maneuver(1, 5., 0., false), (None, false));
        assert_eq!(lever_maneuver(1, 20., 10., false), (Some(LeverChoice::LaneChange), false));
        assert_eq!(lever_maneuver(1, 5., 10., false), (Some(LeverChoice::LaneChange), true));
        assert_eq!(lever_maneuver(2, 5., 10., true), (Some(LeverChoice::Turn), false));
        let mut gesture = LeverForcedTurn::default();
        assert!(!gesture.update(LEVER_ONE_TOUCH, 1));
        assert!(!gesture.update(LEVER_ONE_TOUCH, 0));
        assert!(!gesture.update(LEVER_ONE_TOUCH, 1));
        assert!(gesture.update(LEVER_LATCHED, 1));
        let mut plain = LeverForcedTurn::default();
        assert!(!plain.update(LEVER_ONE_TOUCH, 1));
        assert!(!plain.update(LEVER_LATCHED, 1));
    }

    #[test]
    fn request_gives_up_after_five_flashes_until_the_lamp_goes_dark() {
        let mut wait = LeverRequestWait::default();
        assert!(!wait.update(true, 1, 1, 1, State::PreLaneChange, 0.));
        assert_eq!(wait.update_hold(State::PreLaneChange, 1), 1);
        let mut gave_up = false;
        for _ in 0..81 {
            gave_up = wait.update(true, 0, 0, 1, State::PreLaneChange, 0.);
        }
        assert!(gave_up);
        assert!(!wait.update(true, 0, 0, 0, State::Off, 0.));
    }

    #[test]
    fn crossing_fires_after_the_line_relabels_outward() {
        let mut lines: [Vec<f64>; 4] = std::array::from_fn(|_| vec![0.]);
        let mut crossing = LaneCrossing::default();
        lines[1][0] = -0.8;
        assert_eq!(crossing.update(&lines, true), -1.);
        lines[1][0] = -2.5;
        assert_eq!(crossing.update(&lines, true), 0.);
    }
}
