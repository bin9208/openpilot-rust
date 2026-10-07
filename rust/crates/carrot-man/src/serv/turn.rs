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
        let (kind, speed, end, start) = match turn_info {
            1 => (
                "turn left",
                turn_speed,
                self.settings.auto_turn_end * turn_speed / 3.6,
                fork_start,
            ),
            2 => (
                "turn right",
                turn_speed,
                self.settings.auto_turn_end * turn_speed / 3.6,
                fork_start,
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
            7 | 8 => ("straight", 1., 5., 1000.),
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
