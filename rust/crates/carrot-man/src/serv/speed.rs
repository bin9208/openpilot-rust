use super::{CarState, CarrotServ, RearEvent};

pub fn current_speed(distance: f64, target: (f64, f64), deceleration: f64) -> f64 {
    let (safe_kph, safe_time) = target;
    let safe_ms = safe_kph / 3.6;
    let distance = distance - safe_ms * safe_time;
    if distance <= 0. {
        return safe_kph;
    }
    let temp = safe_ms.powi(2) + 2. * deceleration * distance;
    let speed = if temp < 0. { safe_ms } else { temp.sqrt() };
    safe_kph.max(250_f64.min(speed * 3.6))
}

pub fn countdown(distance: f64, previous: (f64, i64), v_ego: f64) -> (i64, f64, bool) {
    if distance <= 0. {
        return (100, 0., false);
    }
    let calculated = num_traits::ToPrimitive::to_i64(
        &(((distance - v_ego).max(1.) / v_ego.max(1.)) + 0.5).trunc(),
    )
    .unwrap_or(i64::MAX);
    let new_target = previous.0 > 0. && distance - previous.0 > 20_f64.max(v_ego * 2.);
    let rearmed = new_target && previous.1 <= 11;
    let left = if new_target {
        calculated
    } else {
        previous.1.min(calculated)
    };
    (if rearmed { 100 } else { left }, distance, rearmed)
}

impl CarrotServ {
    pub fn external_safety(&self) -> bool {
        if !self.speed.external_active || self.settings.control_mode <= 0 || self.off_route {
            return false;
        }
        if self.nav.speed_type >= 0
            && self.nav.speed_limit > 0.
            && (self.nav.speed_distance > 0.
                || (matches!(self.nav.speed_type, 100 | 101) && self.nav.speed_distance > -250.))
        {
            return true;
        }
        self.settings.rear_hold_distance > 0.
            && self
                .speed
                .rear_events
                .iter()
                .any(|e| e.target + self.settings.rear_hold_distance > self.speed.total_distance)
    }
    pub fn bump_active(&self, distance: f64) -> bool {
        distance > self.settings.bump_end_distance
    }
    pub fn vehicle_camera(&self, car: &CarState) -> bool {
        !self.external_safety()
            && self.settings.camera_control_mode > 0
            && car.speed_limit > 0.
            && car.speed_limit_distance > 0.
            && !(car.school_zone_active && self.speed.school_suppressed)
            && !(self.settings.camera_control_mode == 3 && car.gas_pressed)
    }
    pub fn vehicle_bump(&self, car: &CarState) -> bool {
        !self.external_safety()
            && self.settings.vehicle_can_control != 0
            && self.settings.control_mode >= 2
            && self.bump_active(car.speed_bump_distance)
    }
    pub fn vehicle_school(&mut self, car: &CarState) -> bool {
        if !car.school_zone_active {
            self.speed.school_override_since = None;
            self.speed.school_suppressed = false;
            return false;
        }
        !self.external_safety()
            && self.settings.school_control
            && self.settings.camera_control_mode > 0
            && !self.speed.school_suppressed
            && !(self.settings.camera_control_mode == 3 && car.gas_pressed)
    }
    pub fn vehicle_section(&self, car: &CarState) -> bool {
        !self.external_safety()
            && self.settings.vehicle_can_control != 0
            && self.settings.camera_control_mode > 0
            && car.vehicle_navi_section_active
            && car.vehicle_navi_speed > 0.
            && !(self.settings.camera_control_mode == 3 && car.gas_pressed)
    }
    pub fn vehicle_display(&self, car: Option<&CarState>) -> (bool, i64, bool) {
        let Some(car) = car else {
            return (false, 0, false);
        };
        if self.external_safety()
            || self.settings.vehicle_can_control == 0
            || !car.vehicle_navi_active
        {
            return (false, 0, false);
        }
        let speed = if car.school_zone_active {
            30.
        } else if car.vehicle_navi_speed > 0. {
            car.vehicle_navi_speed * self.settings.safety_factor
        } else if self.bump_active(car.speed_bump_distance) {
            self.settings.bump_speed
        } else {
            0.
        };
        let speed = num_traits::ToPrimitive::to_i64(&speed.trunc()).unwrap_or(0);
        (speed > 0, speed, car.vehicle_navi_section_active)
    }
    pub fn update_external(&mut self) -> bool {
        let active =
            self.navi_active || self.nav.active_count > 0 || self.nav.active_kisa_count > 0;
        let changed = active != self.speed.external_active;
        self.speed.external_active = active;
        if changed {
            self.speed.rear_events.clear();
            self.speed.speed_countdown_distance = 0.;
            self.speed.turn_countdown_distance = 0.;
            self.speed.left_speed_seconds = 100;
            self.speed.left_turn_seconds = 100;
            self.speed.gas_override = 0.;
            self.speed.school_override_since = None;
            self.speed.school_suppressed = false;
        }
        changed
    }
    pub fn rear_camera(&mut self, car: Option<&CarState>, delta: f64) -> (f64, f64) {
        let hold = self.settings.rear_hold_distance;
        if car.is_none()
            || delta < 0.
            || !self.speed.external_active
            || self.settings.control_mode <= 0
            || hold <= 0.
            || self.off_route
        {
            self.speed.rear_events.clear();
            return (250., 0.);
        }
        self.speed
            .rear_events
            .retain(|e| e.target + hold > self.speed.total_distance);
        if self.nav.active_carrot > 1
            && matches!(self.nav.speed_type, 75 | 76)
            && self.nav.speed_distance > 0.
            && self.nav.speed_distance <= 50.
            && self.nav.speed_limit > 0.
        {
            let target = self.speed.total_distance + self.nav.speed_distance;
            if let Some(event) = self
                .speed
                .rear_events
                .iter_mut()
                .find(|e| (e.target - target).abs() <= 40.)
            {
                event.speed = event.speed.min(self.nav.speed_limit);
            } else {
                self.speed.rear_events.push(RearEvent {
                    target,
                    speed: self.nav.speed_limit,
                });
            }
        }
        self.speed
            .rear_events
            .iter()
            .min_by(|a, b| a.speed.total_cmp(&b.speed))
            .map_or((250., 0.), |e| {
                (
                    e.speed,
                    (e.target + hold - self.speed.total_distance).max(0.),
                )
            })
    }
    pub fn countdown_distance(&self, car: Option<&CarState>) -> f64 {
        let mut distance = f64::INFINITY;
        if self.speed.external_active
            && self.nav.speed_distance > 0.
            && !(self.nav.speed_type == 22 && self.settings.countdown_mode == 1)
        {
            distance = self.nav.speed_distance;
        }
        if let Some(car) = car.filter(|c| {
            !self.external_safety()
                && self.settings.vehicle_can_control != 0
                && c.vehicle_navi_active
        }) {
            if car.speed_limit_distance > 0. {
                distance = distance.min(car.speed_limit_distance);
            }
            if self.settings.countdown_mode >= 2 && car.speed_bump_distance > 0. {
                distance = distance.min(car.speed_bump_distance);
            }
        }
        if distance.is_infinite() {
            0.
        } else {
            distance
        }
    }
    pub fn countdown_alert(&mut self, left: i64, source: &str, ego_kph: f64) {
        if left > 11 {
            self.speed.left_seconds = 100;
            self.speed.max_left_seconds = 100;
            self.speed.carrot_left_seconds = 100;
            self.speed.sdi_inform = false;
            return;
        }
        self.speed.sdi_inform = matches!(source, "cam" | "hda");
        self.speed.max_left_seconds =
            (num_traits::ToPrimitive::to_i64(&(ego_kph / 10.).trunc()).unwrap_or(0) + 1)
                .clamp(6, 11);
        if left != self.speed.left_seconds {
            if left == self.speed.max_left_seconds && self.speed.sdi_inform {
                self.speed.carrot_left_seconds = 11;
            } else if (1..self.speed.max_left_seconds).contains(&left)
                || (left == 0 && self.speed.left_seconds == 1)
            {
                self.speed.carrot_left_seconds = left;
            }
            self.speed.left_seconds = left;
        }
    }
}
