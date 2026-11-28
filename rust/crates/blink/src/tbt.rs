//! TBT-only physical right indicator request; never a steering or lane
//! command. Port of carrot's `tools/jetlink/blink_tbt.py`.
use super::observed::Meta;
use openpilot_desire::types::{Car, Lead, Model};

pub const CONFIRM_SEC: f64 = 1.0;
pub const REPEAT_DELAY_SEC: f64 = 2.0;
pub const START_TIMEOUT_SEC: f64 = 4.0;
pub const MANEUVER_TIMEOUT_SEC: f64 = 15.0;
pub const TBT_DISTANCE_MAX_M: f64 = 250.0;
pub const FRONT_GAP_MIN_M: f64 = 5.0;
pub const FRONT_PREDICT_SEC: f64 = 3.0;
pub const INDICATOR_SPEED_MIN: f64 = 10. / 3.6;
const LANE_CHANGE_SPEED_MIN: f64 = 30. / 3.6;

/// Vehicle inputs only the TBT trigger reads.
#[derive(Clone, Copy, Debug, Default)]
pub struct TbtCar {
    pub rear_inputs_available: bool,
    pub right_rear_blocked: bool,
    pub right_blindspot: bool,
}

/// Navigation inputs (`carrotMan`).
#[derive(Clone, Debug, Default)]
pub struct Nav {
    pub turn_info: i32,
    pub distance: f64,
    pub atc_type: String,
}

/// Checks the full target-lane list: `leadRight` omits gaps within 5 m.
/// `errors` is any radar error flag set.
pub fn right_front_clear(errors: bool, leads: &[Lead]) -> bool {
    if errors {
        return false;
    }
    for lead in leads.iter().filter(|l| l.status) {
        let distance = lead.d_rel;
        if !distance.is_finite() {
            return false;
        }
        if distance < 0. {
            continue; // Rear admission is OEM BSD.
        }
        let Some(relative) = lead.v_rel.filter(|v| v.is_finite()) else {
            return false;
        };
        if distance < FRONT_GAP_MIN_M || distance + relative * FRONT_PREDICT_SEC < FRONT_GAP_MIN_M {
            return false;
        }
    }
    true
}

pub fn candidate(
    car: &Car,
    tbt: TbtCar,
    model: &Model,
    meta: &Meta,
    nav: &Nav,
    limit: f64,
) -> bool {
    let edges_ok =
        [0, 4, 8].iter().all(
            |&i| match (model.road_edges[1].get(i), model.lane_lines[2].get(i)) {
                (Some(&edge), Some(&line)) => edge.is_finite() && edge - line >= 2.5,
                _ => false,
            },
        );
    matches!(nav.turn_info, 2 | 4)
        && nav.distance.is_finite()
        && 0. < nav.distance
        && nav.distance <= limit
        && car.v_ego.is_finite()
        && car.v_ego >= INDICATOR_SPEED_MIN
        && car.drive
        && car.can_valid
        && !car.trailer_connected
        && !car.steering_pressed
        && !car.steer_fault_temporary
        && !car.steer_fault_permanent
        && tbt.rear_inputs_available
        && !tbt.right_rear_blocked
        && meta.lane_change_available_right
        && 2.5 < meta.lane_width_right
        && meta.lane_width_right < 5.0
        && model.lane_line_probs[2] > 0.5
        && model.lane_line_probs[3] > 0.5
        && edges_ok
        && car.right_lane_line >= 0
        && matches!(car.right_lane_line % 10, 0 | 5)
        && car.right_lane_line / 10 != 2
        && model.desire_state[1] + model.desire_state[2] <= 0.1
}

pub struct TbtInput<'a> {
    pub now: f64,
    pub enabled: bool,
    pub valid: bool,
    pub car: &'a Car,
    pub tbt: TbtCar,
    /// `None` when the model or navigation is stale.
    pub model: Option<(&'a Model, &'a Meta)>,
    pub nav: Option<&'a Nav>,
    pub manual: bool,
    pub busy: bool,
    pub front_clear: bool,
    pub distance_limit: f64,
}

#[derive(Debug)]
pub struct TbtBlinker {
    pub phase: &'static str,
    ready_since: Option<f64>,
    pub started: f64,
    completed: f64,
    last_distance: Option<f64>,
    last_time: Option<f64>,
    pub reason: &'static str,
    display_only: bool,
}

impl Default for TbtBlinker {
    fn default() -> Self {
        Self {
            phase: "idle",
            ready_since: None,
            started: 0.,
            completed: 0.,
            last_distance: None,
            last_time: None,
            reason: "idle",
            display_only: false,
        }
    }
}

type Request = Option<(&'static str, super::Side)>;

impl TbtBlinker {
    fn cancel(&mut self, reason: &'static str) -> Request {
        self.phase = "cancelled";
        self.ready_since = None;
        self.reason = reason;
        None
    }

    fn owned(&self) -> bool {
        matches!(self.phase, "waiting" | "changing" | "finishing")
    }

    fn request(&self) -> Request {
        Some((
            if self.display_only {
                "tbt_display"
            } else {
                "lane"
            },
            super::Side::Right,
        ))
    }

    pub fn update(&mut self, i: TbtInput<'_>) -> Request {
        // Gaps reset confirmation and cancel an owned indication, never replay it.
        let gap = self
            .last_time
            .is_some_and(|t| !(0. ..=0.35).contains(&(i.now - t)));
        self.last_time = Some(i.now);
        if !i.enabled
            || !i.distance_limit.is_finite()
            || !(0. < i.distance_limit && i.distance_limit <= 500.)
        {
            self.phase = "idle";
            self.ready_since = None;
            self.last_distance = None;
            self.reason = "disabled";
            return None;
        }
        // `valid` already requires fresh model and navigation in carrot.
        let (true, false, Some((model, meta)), Some(nav)) = (i.valid, gap, i.model, i.nav) else {
            self.ready_since = None;
            return if self.owned() {
                self.cancel("inputs")
            } else {
                None
            };
        };
        let distance = nav.distance;
        if !matches!(nav.turn_info, 2 | 4)
            || !distance.is_finite()
            || distance <= 0.
            || distance > i.distance_limit
        {
            self.phase = "idle";
            self.ready_since = None;
            self.last_distance = None;
            self.reason = "route";
            return None;
        }
        if matches!(meta.desire, "turnLeft" | "turnRight") {
            return self.cancel("turn_started");
        }
        let previous = self.last_distance.replace(distance);
        if previous.is_some_and(|p| distance > p + 30.) {
            return self.cancel("route_jump");
        }
        let car = i.car;
        if i.manual || car.steering_pressed || car.left_blinker {
            if self.phase == "idle" {
                // A correction must not disable this TBT; require a new confirmation.
                self.ready_since = None;
                self.reason = "driver";
                return None;
            }
            return self.cancel("driver");
        }
        if i.busy && self.owned() {
            return self.cancel("atc");
        }
        if self.phase == "cancelled" {
            return None;
        }
        let (state, direction) = (meta.lane_change_state, meta.lane_change_direction);
        let active = matches!(state, "laneChangeStarting" | "laneChangeFinishing");
        if self.owned() {
            if i.now - self.started > MANEUVER_TIMEOUT_SEC || (active && direction != "right") {
                return self.cancel("maneuver");
            }
            if active {
                self.phase = if state == "laneChangeFinishing" {
                    "finishing"
                } else {
                    "changing"
                };
            } else if self.phase == "finishing" {
                self.phase = "cooldown";
                self.completed = i.now;
                self.ready_since = None;
                self.reason = "completed";
                return None;
            } else if self.phase == "changing" {
                return self.cancel("interrupted");
            } else if (i.now - self.started >= START_TIMEOUT_SEC && !self.display_only)
                || !i.front_clear
                || !candidate(car, i.tbt, model, meta, nav, i.distance_limit)
            {
                return self.cancel("start");
            }
            self.reason = self.phase;
            return self.request();
        }
        if self.phase == "cooldown" && i.now - self.completed < REPEAT_DELAY_SEC {
            return None;
        }
        // Existing ATC preparation is not competing: the add-on may light the
        // lamp while that same right-turn preparation waits.
        let preparing_right = state == "preLaneChange"
            && direction == "right"
            && matches!(nav.atc_type.as_str(), "atc right" | "fork right");
        if (state != "off" && !preparing_right)
            || car.left_blinker
            || car.right_blinker
            || i.busy
            || !i.front_clear
            || !candidate(car, i.tbt, model, meta, nav, i.distance_limit)
        {
            self.ready_since = None;
            self.reason = "waiting_conditions";
            return None;
        }
        let since = *self.ready_since.get_or_insert(i.now);
        if i.now - since < CONFIRM_SEC {
            self.reason = "confirming";
            return None;
        }
        self.phase = "waiting";
        self.started = i.now;
        self.reason = "trigger";
        self.display_only = car.v_ego < LANE_CHANGE_SPEED_MIN;
        self.request()
    }
}
