use super::{CarState, CarrotServ, TurnResult};

pub fn turn_mapping(kind: i64, instruction: bool) -> (&'static str, &'static str, i64) {
    match kind {
        12 => ("turn", "left", 1),
        16 => ("turn", "sharp left", 1),
        13 => ("turn", "right", 2),
        19 => ("turn", "sharp right", 2),
        1000 if instruction => ("turn", "slight left", 1),
        1001 if instruction => ("turn", "slight right", 2),
        1002 if instruction => ("fork", "slight left", 3),
        1003 if instruction => ("fork", "slight right", 4),
        1006 if instruction => ("off ramp", "left", 3),
        1007 if instruction => ("off ramp", "right", 4),
        102 | 105 | 112 | 115 => ("off ramp", "slight left", 3),
        101 | 104 | 111 | 114 => ("off ramp", "slight right", 4),
        7 | 44 | 17 | 75 | 76 | 118 => ("fork", "left", 3),
        6 | 43 | 73 | 74 | 123 | 124 | 117 => ("fork", "right", 4),
        131 | 132 => ("rotary", "slight right", 5),
        140 | 141 => ("rotary", "slight left", 5),
        133 => ("rotary", "right", 5),
        134 | 135 => ("rotary", "sharp right", 5),
        136..=138 => ("rotary", "sharp left", 5),
        139 => ("rotary", "left", 5),
        142 => ("rotary", "straight", 5),
        14 => ("turn", "uturn", if instruction { 5 } else { 7 }),
        201 => ("arrive", "straight", if instruction { 5 } else { 8 }),
        51..=55 => ("notification", "straight", 0),
        153 | 154 | 249 => ("", "", 6),
        _ => ("invalid", "", -1),
    }
}
fn interpolate(x: f64, bp: &[f64], values: &[f64]) -> f64 {
    if x <= bp[0] {
        return values[0];
    }
    for i in 1..bp.len() {
        if x < bp[i] {
            return values[i - 1]
                + (x - bp[i - 1]) / (bp[i] - bp[i - 1]) * (values[i] - values[i - 1]);
        }
    }
    values[values.len() - 1]
}
/// Metres before a U-turn where ATC announces it with the turn chime.
const UTURN_NOTICE: f64 = 150.;

/// Seconds of travel per lane to cross before a turn.
const LANE_CHANGE_S: f64 = 5.;
const LANE_START_MAX: f64 = 600.;

/// Where a turn's ATC lane changes start. carrot starts them 160-350 m before
/// a turn by road limit whatever the road's width; on a road of `lanes` lanes
/// (node-link, AtcLaneCount) each lane beyond two adds LANE_CHANGE_S of travel
/// at the road limit (4 lanes at 50 km/h: 200 -> 339 m).
pub fn lane_start(fork_start: f64, lanes: Option<u8>, road_limit: f64) -> f64 {
    match lanes {
        Some(n) if n >= 3 => (fork_start
            + f64::from(n - 2) * road_limit.max(30.) / 3.6 * LANE_CHANGE_S)
            .clamp(fork_start, LANE_START_MAX.max(fork_start)),
        _ => fork_start,
    }
}

impl CarrotServ {
    pub fn update_tbt(&mut self) {
        self.nav.turn_info = turn_mapping(self.nav.turn_type, false).2;
        self.nav.next_turn_info = turn_mapping(self.nav.next_turn_type, false).2;
        if self.nav.turn_distance > 0. && self.nav.turn_info > 0 {
            self.nav.x_turn_distance = self.nav.turn_distance;
        }
        if self.nav.next_turn_distance > 0. && self.nav.next_turn_info > 0 {
            self.nav.x_next_turn_distance = self.nav.next_turn_distance + self.nav.turn_distance;
        }
    }
    pub fn auto_turn(
        &mut self,
        turn: (i64, f64),
        car: Option<&CarState>,
        check: bool,
    ) -> TurnResult {
        let (turn_info, distance) = turn;
        let turn_speed = self.settings.auto_turn_speed;
        let fork_speed = self.nav.road_limit;
        let fork_start = interpolate(self.nav.road_limit, &[30., 50., 100.], &[160., 200., 350.]);
        let turn_start = interpolate(
            num_traits::ToPrimitive::to_f64(&self.nav.next_road_width).unwrap_or(0.),
            &[5., 10.],
            &[43., 60.],
        );
        let lanes = if check && self.settings.atc_lane_count {
            self.nav.atc_lanes.map(|(here, _)| here)
        } else {
            None
        };
        let turn_lane_start = lane_start(fork_start, lanes, self.nav.road_limit);
        let (kind, speed, end, start) = match turn_info {
            1 => (
                "turn left",
                turn_speed,
                self.settings.auto_turn_end * turn_speed / 3.6,
                turn_lane_start,
            ),
            2 => (
                "turn right",
                turn_speed,
                self.settings.auto_turn_end * turn_speed / 3.6,
                turn_lane_start,
            ),
            5 => (
                "straight",
                turn_speed,
                self.settings.auto_turn_end * turn_speed / 3.6,
                turn_start,
            ),
            3 => (
                "fork left",
                fork_speed,
                self.settings.auto_turn_end * fork_speed / 3.6,
                fork_start,
            ),
            4 => (
                "fork right",
                fork_speed,
                self.settings.auto_turn_end * fork_speed / 3.6,
                fork_start,
            ),
            6 => (
                "straight",
                fork_speed,
                self.settings.auto_turn_end * fork_speed / 3.6,
                fork_start,
            ),
            // U-turn: openpilot cannot steer one; the driver turns. carrot slowed
            // to 1 km/h, a crawl in the middle of the lane; ATC now slows to the
            // turn speed and chimes UTURN_NOTICE m before (kind "uturn").
            7 => (
                "uturn",
                turn_speed,
                self.settings.auto_turn_end * turn_speed / 3.6,
                fork_start,
            ),
            // Arrival: no slowdown. The destination may be mid-lane or inside a
            // car park; the navigation announces it and the driver stops.
            8 => ("none", 0., 0., 1000.),
            _ => ("none", 0., 0., 1000.),
        };
        let mut kind = kind.to_owned();
        if distance > start {
            kind.push_str(" prepare");
            if check {
                self.speed.atc_activate_count = (self.speed.atc_activate_count - 1).min(0);
            }
        } else {
            if check {
                self.speed.atc_activate_count = (self.speed.atc_activate_count + 1).max(0);
            }
            // The chime plays on a change into a "turn" kind from one without it.
            if kind == "uturn" && distance > UTURN_NOTICE.max(turn_start) {
                kind = "straight".into();
            }
            if matches!(kind.as_str(), "turn left" | "turn right") && distance > turn_start {
                kind = if kind == "turn left" {
                    "atc left"
                } else {
                    "atc right"
                }
                .into();
            }
        }
        if self.settings.auto_turn_map_change > 0 && check {
            match self.speed.atc_activate_count {
                2 => {
                    self.command.command_index += 100;
                    self.command.command = "DISPLAY".into();
                    self.command.argument = "MAP".into();
                    self.command.command_text = true;
                    self.command.argument_text = true;
                    self.command.command_hashable = true;
                }
                -50 => {
                    self.command.command_index += 100;
                    self.command.command = "DISPLAY".into();
                    self.command.argument = "ROAD".into();
                    self.command.command_text = true;
                    self.command.argument_text = true;
                    self.command.command_hashable = true;
                }
                _ => {}
            }
        }
        if check {
            if distance >= 0.
                && distance < start
                && matches!(kind.as_str(), "fork left" | "fork right")
            {
                if !self.speed.atc_paused {
                    if let Some(car) = car {
                        if car.steering_pressed
                            && ((car.steering_torque < 0.
                                && matches!(kind.as_str(), "fork left" | "atc left"))
                                || (car.steering_torque > 0.
                                    && matches!(kind.as_str(), "fork right" | "atc right")))
                        {
                            self.speed.atc_paused = true;
                        }
                    }
                }
            } else {
                self.speed.atc_paused = false;
            }
            if self.speed.atc_paused {
                kind.push_str(" canceled");
            }
        }
        let desired = if speed > 0. && distance > 0. {
            250_f64.min(super::speed::current_speed(
                distance - end,
                (speed, 2.),
                self.settings.deceleration,
            ))
        } else {
            250.
        };
        TurnResult {
            desired,
            kind,
            speed,
            distance: end,
        }
    }
}

#[cfg(test)]
mod lane_start_tests {
    use super::lane_start;

    #[test]
    fn wide_roads_start_the_lane_changes_earlier() {
        assert_eq!(lane_start(200., None, 50.), 200.);
        assert_eq!(lane_start(200., Some(2), 50.), 200.);
        assert!((lane_start(200., Some(4), 50.) - 338.9).abs() < 0.1);
        assert_eq!(lane_start(350., Some(8), 100.), 600.);
        assert_eq!(lane_start(200., Some(3), 0.), 200. + 30. / 3.6 * 5.);
    }
}

#[cfg(test)]
mod uturn_arrive_tests {
    use crate::serv::{settings::Settings, CarrotServ};

    fn serv() -> CarrotServ {
        let mut settings: Settings = serde_json::from_value(serde_json::json!({"bump_speed":0.,"bump_time":0.,"bump_end_distance":0.,"control_end":0.,"rear_hold_distance":0.,"control_mode":0,"vehicle_can_control":0,"school_control":false,"camera_control_mode":0,"safety_factor":0.,"deceleration":1.,"countdown_mode":0,"turn_speed_mode":0,"map_turn_factor":0.,"auto_turn_speed":20.,"auto_turn_map_change":0,"auto_turn_control":2,"auto_turn_end":3.,"curve_lower_limit":0.,"is_metric":true,"road_limit_offset":0.,"curve_factor":0.,"language":"en"})).unwrap();
        settings.atc_lane_count = false;
        let mut serv = CarrotServ::new(settings);
        serv.nav.road_limit = 50.;
        serv
    }

    #[test]
    fn a_uturn_chimes_near_and_slows_only_to_the_turn_speed() {
        let mut s = serv();
        assert_eq!(s.auto_turn((7, 400.), None, false).kind, "uturn prepare");
        assert_eq!(s.auto_turn((7, 180.), None, false).kind, "straight");
        let near = s.auto_turn((7, 100.), None, false);
        assert_eq!(near.kind, "uturn");
        let at = s.auto_turn((7, 1.), None, false);
        assert!(
            at.desired >= 19.,
            "turn speed, not a 1 km/h crawl: {}",
            at.desired
        );
    }

    #[test]
    fn arrival_neither_slows_nor_signals() {
        let mut s = serv();
        for distance in [300., 50., 5.] {
            let result = s.auto_turn((8, distance), None, false);
            assert!(result.kind.starts_with("none"), "{}", result.kind);
            assert_eq!(result.desired, 250.);
        }
    }
}
