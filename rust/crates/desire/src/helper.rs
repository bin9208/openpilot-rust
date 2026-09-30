use crate::{
    side::SideState,
    types::{
        maximum, minimum, Config, Input, InvalidModel, Maneuver, Navigation, State, DT, MIN_SPEED,
    },
};
use serde::Serialize;

#[derive(Serialize)]
pub struct DesireHelper {
    pub frame: u64,
    pub lane_change_state: State,
    pub lane_change_direction: u8,
    pub lane_change_timer: f64,
    pub lane_change_ll_prob: f64,
    pub lane_change_delay: f64,
    pub maneuver_type: Maneuver,
    pub desire: u8,
    pub turn_direction: u8,
    pub enable_turn_desires: bool,
    pub turn_desire_state: bool,
    pub desire_disable_count: u32,
    pub turn_disable_count: u32,
    pub left: SideState,
    pub right: SideState,
    pub blinker_ignore: bool,
    pub driver_blinker_state: u8,
    pub carrot_blinker_state: u8,
    pub carrot_lane_change_count: u32,
    pub carrot_cmd_index_last: i64,
    pub atc_type: String,
    pub atc_active: u8,
    pub auto_lane_change_enable: bool,
    pub next_lane_change: bool,
    pub keep_pulse_timer: f64,
    pub config: Config,
    pub prev_desire_enabled: bool,
    pub desire_log: String,
    pub lane_change_available_left: bool,
    pub lane_change_available_right: bool,
}

impl Default for DesireHelper {
    fn default() -> Self {
        Self {
            frame: 0,
            lane_change_state: State::Off,
            lane_change_direction: 0,
            lane_change_timer: 0.0,
            lane_change_ll_prob: 1.0,
            lane_change_delay: 0.0,
            maneuver_type: Maneuver::None,
            desire: 0,
            turn_direction: 0,
            enable_turn_desires: true,
            turn_desire_state: false,
            desire_disable_count: 0,
            turn_disable_count: 0,
            left: SideState::new("left"),
            right: SideState::new("right"),
            blinker_ignore: false,
            driver_blinker_state: 0,
            carrot_blinker_state: 0,
            carrot_lane_change_count: 0,
            carrot_cmd_index_last: 0,
            atc_type: String::new(),
            atc_active: 0,
            auto_lane_change_enable: false,
            next_lane_change: false,
            keep_pulse_timer: 0.0,
            config: Config::default(),
            prev_desire_enabled: false,
            desire_log: String::new(),
            lane_change_available_left: false,
            lane_change_available_right: false,
        }
    }
}

impl DesireHelper {
    fn atc_blinker(
        &mut self,
        navigation: &Navigation,
        driver: u8,
        remote: Option<&str>,
    ) -> (u8, bool) {
        let kind = navigation.atc_type.as_str();
        let mut blinker = 0;
        if self.carrot_lane_change_count > 0 {
            blinker = self.carrot_blinker_state;
        } else if matches!(remote, Some("laneLeft" | "laneRight")) {
            self.carrot_lane_change_count = 4;
            self.carrot_blinker_state = if remote == Some("laneLeft") { 1 } else { 2 };
            blinker = self.carrot_blinker_state;
        } else if navigation.command_index != self.carrot_cmd_index_last
            && navigation.command == "LANECHANGE"
        {
            self.carrot_cmd_index_last = navigation.command_index;
            self.carrot_lane_change_count = 4;
            self.carrot_blinker_state = if navigation.argument == "LEFT" { 1 } else { 2 };
            blinker = self.carrot_blinker_state;
        } else if matches!(kind, "turn left" | "turn right") {
            if self.atc_active != 2 {
                blinker = if kind == "turn left" { 1 } else { 2 };
                self.atc_active = 1;
                self.blinker_ignore = false;
            }
        } else if matches!(kind, "fork left" | "fork right" | "atc left" | "atc right") {
            if self.atc_active != 2 {
                blinker = if matches!(kind, "fork left" | "atc left") {
                    1
                } else {
                    2
                };
                self.atc_active = 1;
            }
        } else {
            self.atc_active = 0;
        }
        if driver != 0 && blinker != 0 && driver != blinker {
            blinker = 0;
            self.atc_active = 2;
        }
        let mut enabled = matches!(blinker, 1 | 2);
        if driver == 0 {
            self.blinker_ignore = false;
        }
        if self.blinker_ignore {
            blinker = 0;
            enabled = false;
        }
        if self.atc_type != kind {
            enabled = false;
        }
        self.atc_type.clone_from(&navigation.atc_type);
        (blinker, enabled)
    }

    fn sides(&mut self, input: &Input) {
        let model = &input.model;
        for (index, side) in [&mut self.left, &mut self.right].into_iter().enumerate() {
            let (outer, current) = if index == 0 { (0, 1) } else { (3, 2) };
            side.update_geometry(
                &model.lane_lines[outer],
                model.lane_line_probs[outer],
                &model.lane_lines[current],
                &model.road_edges[index],
                model.lane_line_probs[current],
            );
            let (line, blindspot) = if index == 0 {
                (input.car.left_lane_line, input.car.left_blindspot)
            } else {
                (input.car.right_lane_line, input.car.right_blindspot)
            };
            side.update_line(line);
            side.update_obstacles(
                input.car.v_ego,
                &input.leads[index],
                blindspot,
                self.config.bsd < 0,
                &input.objects[index],
            );
            let line_allowed = if self.config.line_check >= 1 {
                matches!(side.lane_line_info_mod, 0 | 5)
            } else {
                side.lane_line_info_raw.div_euclid(10) != 2
            };
            side.compute_available(line_allowed, self.config.bsd < 0);
            side.update_triggers();
        }
        self.lane_change_available_left = self.left.lane_change_available;
        self.lane_change_available_right = self.right.lane_change_available;
    }

    pub fn update(
        &mut self,
        input: &Input,
        mut config: impl FnMut() -> Config,
        mut command: impl FnMut(bool) -> Option<String>,
    ) -> Result<u8, InvalidModel> {
        input.validate()?;
        self.frame = self.frame.wrapping_add(1);
        if self.frame.is_multiple_of(100) {
            self.config = config();
        }
        self.carrot_lane_change_count = self.carrot_lane_change_count.saturating_sub(1);
        self.lane_change_delay = maximum(0.0, self.lane_change_delay - DT);
        let car = &input.car;
        let below_speed = car.v_ego < MIN_SPEED;
        let trailer = car.trailer_connected;
        self.sides(input);
        if trailer {
            self.left.lane_change_available = false;
            self.right.lane_change_available = false;
            self.lane_change_available_left = false;
            self.lane_change_available_right = false;
            self.auto_lane_change_enable = false;
            self.next_lane_change = false;
            self.desire_log = "TRAILER:MANEUVER_BLOCKED".to_owned();
        }
        self.turn_desire_state = input.model.desire_state[1] + input.model.desire_state[2] > 0.1;
        if self.maneuver_type == Maneuver::Turn
            && car.steering_angle_deg.abs() > 80.0
            && input.model.orientation_rate_z[15].abs() < input.model.orientation_rate_z[5].abs()
        {
            self.turn_disable_count = 200;
        } else {
            self.turn_disable_count = self.turn_disable_count.saturating_sub(1);
        }
        let driver = u8::from(car.left_blinker) + 2 * u8::from(car.right_blinker);
        let driver_changed = driver != self.driver_blinker_state;
        self.driver_blinker_state = driver;
        let driver_enabled = matches!(driver, 1 | 2) && self.config.need_torque >= 0;
        let remote = command(input.lateral_active && car.can_valid && !below_speed && !trailer);
        let (atc, atc_enabled) = self.atc_blinker(&input.navigation, driver, remote.as_deref());
        let enabled = driver_enabled || atc_enabled;
        let blinker = if driver_enabled { driver } else { atc };
        let side = match blinker {
            1 => Some(&self.left),
            2 => Some(&self.right),
            _ => None,
        };
        let atc_requested = atc_enabled
            && matches!(
                self.atc_type.as_str(),
                "fork left" | "fork right" | "atc left" | "atc right"
            );
        let atc_manual = atc_enabled
            && !driver_enabled
            && matches!(self.atc_type.as_str(), "fork left" | "atc left");
        let atc_only = atc_requested && !driver_enabled;
        let retry_blocked =
            atc_only && side.is_some_and(|side| !matches!(side.lane_line_info_mod, 0 | 5));
        let mut auto_trigger = false;
        if let Some(side) = side.filter(|_| enabled && !trailer) {
            auto_trigger = if self.carrot_lane_change_count > 0 {
                side.lane_change_available
            } else {
                self.auto_lane_change_enable
                    && !atc_manual
                    && side.edge_available
                    && (side.lane_available_trigger || side.lane_appeared)
                    && !side.side_object_detected
                    && side.bsd_hold_counter == 0
            };
            self.desire_log = format!(
                "{}:ALC={}, ",
                side.name,
                if self.auto_lane_change_enable {
                    "True"
                } else {
                    "False"
                }
            );
        } else {
            self.auto_lane_change_enable = false;
            self.next_lane_change = false;
        }
        if !input.lateral_active
            || self.lane_change_timer > 10.0
            || trailer
            || self.desire_disable_count > 0
        {
            self.lane_change_state = State::Off;
            self.lane_change_direction = 0;
            self.turn_direction = 0;
            self.maneuver_type = Maneuver::None;
        } else {
            let new_type = match side.filter(|_| enabled) {
                Some(side) => classify(
                    input,
                    side,
                    self.turn_desire_state,
                    &self.atc_type,
                    self.maneuver_type,
                ),
                None => Maneuver::None,
            };
            if self.maneuver_type == Maneuver::LaneChange
                && new_type == Maneuver::Turn
                && !matches!(
                    self.lane_change_state,
                    State::PreLaneChange | State::Starting
                )
            {
                self.maneuver_type = Maneuver::Turn;
                self.lane_change_state = State::Off;
            } else if matches!(self.lane_change_state, State::Off | State::PreLaneChange) {
                self.maneuver_type = new_type;
            }
            if enabled && self.maneuver_type == Maneuver::Turn && self.enable_turn_desires {
                self.lane_change_state = State::Off;
                if self.turn_disable_count > 0 {
                    self.turn_direction = 0;
                    self.lane_change_direction = 0;
                } else {
                    self.turn_direction = if blinker == 1 { 1 } else { 2 };
                    self.lane_change_direction = self.turn_direction;
                }
            } else {
                self.turn_direction = 0;
                match self.lane_change_state {
                    State::Off => {
                        if let Some(side) = side.filter(|_| {
                            enabled
                                && (!self.prev_desire_enabled || (driver_enabled && driver_changed))
                                && !below_speed
                        }) {
                            self.lane_change_state = State::PreLaneChange;
                            self.lane_change_ll_prob = 1.0;
                            self.lane_change_delay = self.config.delay_tenths * 0.1;
                            self.auto_lane_change_enable = last_lane(side);
                            self.next_lane_change = false;
                        }
                    }
                    State::PreLaneChange => {
                        if let Some(side) = side {
                            self.lane_change_direction = if blinker == 1 { 1 } else { 2 };
                            let torque = car.steering_pressed
                                && if blinker == 1 {
                                    car.steering_torque > 0.0
                                } else {
                                    car.steering_torque < 0.0
                                };
                            let bsd_active = side.bsd_hold_counter > 0 && self.config.bsd >= 0;
                            let clear = (side.lane_available || side.edge_available)
                                && !side.side_object_detected
                                && !bsd_active;
                            let line_release = ((atc_requested && driver_enabled)
                                || (atc_only && auto_trigger))
                                && clear;
                            if atc_only && last_lane(side) {
                                self.auto_lane_change_enable = true;
                            }
                            if !enabled || below_speed {
                                self.lane_change_state = State::Off;
                                self.lane_change_direction = 0;
                            } else {
                                let solid = self.config.line_check >= 2
                                    && !side.lane_change_available_geom
                                    && (side.lane_available || side.edge_available);
                                let block_released = side.lane_change_available_released
                                    && (driver_enabled || self.auto_lane_change_enable)
                                    && !retry_blocked;
                                let gate = (side.lane_change_available_geom
                                    && self.lane_change_delay == 0.0)
                                    || side.lane_line_info_edge_detect
                                    || solid
                                    || block_released
                                    || line_release;
                                let start = if solid {
                                    line_release
                                        || (torque && !(bsd_active && self.config.bsd == 1))
                                } else if bsd_active {
                                    torque && self.config.bsd != 1
                                } else if self.config.need_torque > 0 || self.next_lane_change {
                                    torque
                                } else if driver_enabled {
                                    side.lane_change_available || line_release
                                } else {
                                    (torque
                                        || (!atc_manual
                                            && (auto_trigger
                                                || side.lane_line_info_edge_detect
                                                || block_released)))
                                        && (side.lane_change_available || line_release)
                                };
                                if gate && start {
                                    self.lane_change_state = State::Starting;
                                }
                            }
                        } else {
                            self.lane_change_state = State::Off;
                            self.lane_change_direction = 0;
                        }
                    }
                    State::Starting => {
                        self.lane_change_ll_prob =
                            maximum(self.lane_change_ll_prob - 2.0 * DT, 0.0);
                        if input.lane_change_prob < 0.02 && self.lane_change_ll_prob < 0.01 {
                            self.lane_change_state = State::Finishing;
                        }
                    }
                    State::Finishing => {
                        self.lane_change_ll_prob = minimum(self.lane_change_ll_prob + DT, 1.0);
                        if self.lane_change_ll_prob > 0.99 {
                            self.lane_change_direction = 0;
                            if enabled {
                                self.lane_change_state = State::PreLaneChange;
                                self.next_lane_change = true;
                            } else {
                                self.lane_change_state = State::Off;
                            }
                        }
                    }
                }
            }
        }
        if matches!(self.lane_change_state, State::Off | State::PreLaneChange) {
            self.lane_change_timer = 0.0;
        } else {
            self.lane_change_timer += DT;
        }
        self.left.commit_last();
        self.right.commit_last();
        self.prev_desire_enabled = enabled;
        let cancel = car.steering_pressed
            && ((car.steering_torque < 0.0 && blinker == 1)
                || (car.steering_torque > 0.0 && blinker == 2));
        if cancel && self.lane_change_state != State::Off {
            self.lane_change_direction = 0;
            self.lane_change_state = State::Off;
            self.blinker_ignore = true;
        }
        self.desire = if self.turn_direction != 0 {
            self.lane_change_direction = self.turn_direction;
            self.turn_direction
        } else if matches!(self.lane_change_state, State::Starting | State::Finishing)
            && self.lane_change_direction != 0
        {
            self.lane_change_direction + 2
        } else {
            0
        };
        if matches!(self.lane_change_state, State::Off | State::Starting) {
            self.keep_pulse_timer = 0.0;
        } else if self.lane_change_state == State::PreLaneChange {
            self.keep_pulse_timer += DT;
            if self.keep_pulse_timer > 1.0 {
                self.keep_pulse_timer = 0.0;
            } else if matches!(self.desire, 5 | 6) {
                self.desire = 0;
            }
        }
        Ok(self.desire)
    }
}

fn last_lane(side: &SideState) -> bool {
    side.lane_exist_count.counter <= 0 && !side.lane_change_available_geom
}

fn classify(
    input: &Input,
    side: &SideState,
    turn_desire: bool,
    atc: &str,
    old: Maneuver,
) -> Maneuver {
    let speed = input.car.v_ego * 3.6;
    let mut score = i32::from(speed < 30.0 || (speed < 40.0 && input.car.a_ego < -1.0));
    score += i32::from(speed < 40.0 && !side.lane_available && !side.edge_available);
    score += i32::from(speed < 40.0 && side.lane_exist_count.counter < 10);
    score += i32::from(turn_desire);
    if matches!(atc, "turn left" | "turn right") {
        score += 2;
    } else if matches!(atc, "fork left" | "fork right" | "atc left" | "atc right") {
        score -= 2;
    }
    if score >= 2 {
        if side.dist_to_edge_far > 4.0 {
            Maneuver::Turn
        } else {
            old
        }
    } else {
        Maneuver::LaneChange
    }
}
