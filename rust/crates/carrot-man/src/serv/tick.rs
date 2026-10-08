use super::{
    speed::{countdown, current_speed},
    turn::turn_mapping,
    CarrotServ, TickInput,
};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Decision {
    pub desired_speed: f64,
    pub source: String,
    pub atc_type: String,
    pub route_speed: f64,
    pub vturn_speed: f64,
    pub published_road_limit: f64,
    pub rear_holding: bool,
    pub rear_remaining: f64,
    pub vehicle_display: (bool, i64, bool),
    pub vehicle_available: bool,
    pub safety_rejection: String,
    pub decel_provider: String,
    pub debug_text: String,
    pub position_speed: f64,
}

impl CarrotServ {
    pub fn tick(&mut self, input: TickInput) -> Decision {
        let car = input.car.as_ref().filter(|_| input.selfdrive_alive);
        let (v_ego, delta) = if let Some(car) = car {
            let delta = input.distance_traveled - self.speed.total_distance;
            self.speed.total_distance = input.distance_traveled;
            (car.v_ego, delta)
        } else {
            (0., 0.)
        };
        self.gps.bearing = self.update_gps(&input.gps, (v_ego, input.now));
        self.nav.speed_distance = (self.nav.speed_distance - delta).max(-1000.);
        if self.settings.control_mode <= 0
            || (self.nav.speed_type == 22 && self.settings.control_mode < 2)
            || (self.nav.speed_type == 7 && self.settings.control_mode < 3)
        {
            self.disabled_safety = self.safety_identity();
            self.nav.speed_type = -1;
            self.nav.speed_limit = 0.;
            self.nav.speed_distance = 0.;
        }
        self.nav.x_turn_distance -= delta;
        self.nav.x_next_turn_distance -= delta;
        self.nav.active_count = (self.nav.active_count - 1).max(0);
        self.nav.active_sdi_count = (self.nav.active_sdi_count - 1).max(0);
        self.nav.active_kisa_count = (self.nav.active_kisa_count - 1).max(0);
        let navigation_changed = self.update_external();
        if !self.speed.external_active {
            if let Some(car) = car.filter(|c| c.speed_limit > 0.) {
                self.nav.road_limit = car.speed_limit;
            }
        }
        let road_changed = self.nav.road_limit != self.nav.last_road_limit;
        self.nav.last_road_limit = self.nav.road_limit;
        self.nav.active_carrot = if self.nav.active_kisa_count > 0 {
            2
        } else if self.nav.active_count > 0 {
            if self.nav.active_sdi_count > 0 {
                2
            } else {
                1
            }
        } else {
            0
        };
        let mut limit_speed = 200.;
        if self.settings.road_limit_offset >= 0.
            && self.nav.active_carrot >= 2
            && (!self.has_control || self.road_limit_valid)
            && self.nav.road_limit >= 30.
        {
            let offset = if self.settings.is_metric {
                self.settings.road_limit_offset
            } else {
                self.settings.road_limit_offset * 0.621371192237334
            };
            limit_speed = self.nav.road_limit + offset;
        }
        if self.nav.active_carrot <= 1 {
            self.nav.speed_type = -1;
            self.nav.turn_info = -1;
            self.nav.next_turn_info = -1;
            self.nav.sdi_type = -1;
            self.nav.block_type = -1;
            self.nav.secondary_block_type = -1;
            self.nav.turn_type = -1;
            self.nav.next_turn_type = -1;
            self.nav.road_category = 8;
            self.nav.goal_distance = 0.;
        }
        let mut debug_text = String::new();
        if self.nav.active_carrot <= 1 || self.nav.active_kisa_count > 0 {
            if let Some(instruction) = &input.nav_instruction {
                self.nav.goal_distance = instruction.distance_remaining.trunc();
                self.nav.goal_time = instruction.time_remaining.trunc();
                if self.nav.active_kisa_count <= 0 && instruction.speed_limit > 0. {
                    self.nav.road_limit =
                        (instruction.speed_limit * 3.6).round_ties_even().max(30.);
                }
                self.nav.x_turn_distance = instruction.maneuver_distance.trunc();
                self.nav.main_text = instruction.primary_text.clone();
                self.main_text_python_json = None;
                self.nav.turn_info = -1;
                for kind in [
                    12, 16, 1000, 1001, 1002, 1003, 1006, 1007, 13, 19, 102, 105, 112, 115, 101,
                    104, 111, 114, 7, 44, 17, 75, 76, 118, 6, 43, 73, 74, 123, 124, 117, 131, 132,
                    140, 141, 133, 134, 135, 136, 137, 138, 139, 142, 14, 201, 51, 52, 53, 54, 55,
                    153, 154, 249,
                ] {
                    let mapping = turn_mapping(kind, true);
                    if mapping.0 == instruction.kind && mapping.1 == instruction.modifier {
                        self.nav.turn_info = mapping.2;
                        break;
                    }
                }
                let display = if self.settings.is_metric {
                    self.nav.road_limit
                } else {
                    self.nav.road_limit * 0.621371192237334
                };
                debug_text = format!(
                    "{display:.0},{},{} ",
                    instruction.kind, instruction.modifier
                );
            }
        }
        let (rear_speed, rear_remaining) = self.rear_camera(car, delta);
        if self.nav.speed_type < 0
            || (!matches!(self.nav.speed_type, 100 | 101) && self.nav.speed_distance <= 0.)
            || (matches!(self.nav.speed_type, 100 | 101) && self.nav.speed_distance < -250.)
        {
            self.nav.speed_type = -1;
            self.nav.speed_distance = 0.;
            self.nav.speed_limit = 0.;
        }
        if self.nav.turn_info < 0 || self.nav.x_turn_distance < -50. {
            if self.nav.x_turn_distance > 0. {
                self.nav.x_turn_distance = 0.;
            }
            self.nav.turn_info = -1;
            self.nav.x_next_turn_distance = 0.;
            self.nav.next_turn_info = -1;
        }
        let mut sdi_speed = 250.;
        let legacy = self.speed.external_active
            && self.nav.speed_limit > 0.
            && (self.nav.speed_distance > 0. || matches!(self.nav.speed_type, 100 | 101))
            && self.nav.active_carrot > 0
            && (self.nav.speed_type != 22 || self.bump_active(self.nav.speed_distance));
        if legacy {
            let safe_sec = if self.nav.speed_type == 22 {
                self.settings.bump_time
            } else {
                self.settings.control_end
            };
            sdi_speed = 250_f64.min(current_speed(
                self.nav.speed_distance,
                (self.nav.speed_limit, safe_sec),
                self.settings.deceleration,
            ));
            self.nav.active_carrot = if self.nav.speed_type == 22 { 5 } else { 3 };
            if self.nav.speed_type == 4
                || (matches!(self.nav.speed_type, 100 | 101) && self.nav.speed_distance <= 0.)
            {
                sdi_speed = self.nav.speed_limit;
                self.nav.active_carrot = 4;
            }
        }
        let mut vehicle_camera = 250.;
        let mut vehicle_bump = 250.;
        let mut school = 250.;
        let mut section = 250.;
        if let Some(car) = car {
            if self.vehicle_camera(car) {
                vehicle_camera = current_speed(
                    car.speed_limit_distance,
                    (
                        car.speed_limit * self.settings.safety_factor,
                        self.settings.control_end,
                    ),
                    self.settings.deceleration,
                );
            }
            if self.vehicle_bump(car) {
                vehicle_bump = current_speed(
                    car.speed_bump_distance,
                    (self.settings.bump_speed, self.settings.bump_time),
                    self.settings.deceleration,
                );
                self.nav.active_carrot = 5;
            }
            if self.vehicle_school(car) {
                school = 30.;
                self.nav.active_carrot = 6;
            }
            if self.vehicle_section(car) {
                section = car.vehicle_navi_speed * self.settings.safety_factor;
                self.nav.active_carrot = 4;
            }
        }
        let mut atc = self.auto_turn((self.nav.turn_info, self.nav.x_turn_distance), car, true);
        let mut next_atc = self.auto_turn(
            (self.nav.next_turn_info, self.nav.x_next_turn_distance),
            car,
            false,
        );
        if !matches!(self.settings.auto_turn_control, 2 | 3) {
            atc.desired = 250.;
            next_atc.desired = 250.;
        }
        if !matches!(self.settings.auto_turn_control, 1 | 2) {
            atc.kind = "none".into();
        }
        let rear_holding = rear_speed < 250. && rear_speed <= sdi_speed;
        sdi_speed = sdi_speed.min(rear_speed);
        if rear_holding {
            self.nav.active_carrot = 3;
        }
        let sdi_source = if rear_holding {
            "cam"
        } else {
            match self.nav.speed_type {
                22 => "bump",
                4 => "section",
                100 => "police",
                101 => "waze",
                _ => "cam",
            }
        };
        let mut candidates = vec![
            (atc.desired, "atc"),
            (next_atc.desired, "atc2"),
            (sdi_speed, sdi_source),
            (vehicle_camera, "hda"),
            (vehicle_bump, "hda_bump"),
            (school, "school"),
            (section, "hda_section"),
            (limit_speed, "road"),
        ];
        if matches!(self.settings.turn_speed_mode, 1 | 2) {
            candidates.push((
                input
                    .vision_speed
                    .abs()
                    .max(self.settings.curve_lower_limit),
                "vturn",
            ));
        }
        let route_speed = (input.route_speed * self.settings.map_turn_factor)
            .max(self.settings.curve_lower_limit);
        if (self.settings.turn_speed_mode == 2
            && self.nav.x_turn_distance > -500.
            && self.nav.x_turn_distance < 500.)
            || matches!(self.settings.turn_speed_mode, 3 | 4)
        {
            candidates.push((route_speed, "route"));
        }
        let (desired, source) = candidates
            .into_iter()
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .unwrap_or((250., "none"));
        let (desired_speed, source) = if let Some(car) = car {
            self.gas_floor(car, desired, source, road_changed, input.now)
        } else {
            (desired, source.into())
        };
        if car.is_some() {
            debug_text.push_str(&format!("route={route_speed:.1}"));
        }
        let (left_speed, speed_distance, speed_rearmed, left_turn, turn_distance, turn_rearmed) =
            if self.settings.countdown_mode > 0 {
                let (speed, sd, sr) = countdown(
                    self.countdown_distance(car),
                    (
                        self.speed.speed_countdown_distance,
                        self.speed.left_speed_seconds,
                    ),
                    v_ego,
                );
                let (turn, td, tr) = countdown(
                    self.nav.x_turn_distance,
                    (
                        self.speed.turn_countdown_distance,
                        self.speed.left_turn_seconds,
                    ),
                    v_ego,
                );
                (speed, sd, sr, turn, td, tr)
            } else {
                (100, 0., false, 100, 0., false)
            };
        self.speed.speed_countdown_distance = speed_distance;
        self.speed.turn_countdown_distance = turn_distance;
        self.speed.left_speed_seconds = left_speed;
        self.speed.left_turn_seconds = left_turn;
        self.countdown_alert(
            if navigation_changed || speed_rearmed || turn_rearmed {
                100
            } else {
                left_speed.min(left_turn)
            },
            &source,
            v_ego * 3.6,
        );
        self.update_command();
        let selection = self.projected.as_ref().map(|p| &p.selection);
        let owner = selection.and_then(|s| s.snapshot.as_ref());
        let road_limit = if owner.is_some_and(|s| s.source == crate::sources::Source::NaverV1)
            && !self.road_limit_valid
        {
            0.
        } else {
            self.nav.road_limit
        };
        let safety_rejection = selection
            .and_then(|s| {
                crate::sources::safety::navigation_safety(
                    s,
                    crate::sources::safety::SafetyPolicy {
                        mode: self.settings.control_mode,
                        safety_factor: self.settings.safety_factor,
                        bump_speed_kph: self.settings.bump_speed,
                    },
                )
                .1
            })
            .unwrap_or(if owner.is_some() && !self.external_safety() {
                "passed"
            } else {
                ""
            })
            .into();
        let provider = if matches!(
            source.as_str(),
            "hda" | "hda_bump" | "hda_section" | "school"
        ) {
            "hda"
        } else if owner.is_some()
            && matches!(
                source.as_str(),
                "cam" | "bump" | "section" | "navi" | "atc" | "atc2" | "route"
            )
        {
            owner.map_or("none", |s| s.source.name())
        } else if source == "vturn" {
            "vision"
        } else {
            "none"
        };
        Decision {
            desired_speed,
            source,
            atc_type: atc.kind,
            route_speed,
            vturn_speed: input.vision_speed,
            published_road_limit: road_limit,
            rear_holding,
            rear_remaining,
            vehicle_display: self.vehicle_display(car),
            vehicle_available: car.is_some_and(|c| c.vehicle_navi_available),
            safety_rejection,
            decel_provider: provider.into(),
            debug_text,
            position_speed: v_ego * 3.6,
        }
    }
}
