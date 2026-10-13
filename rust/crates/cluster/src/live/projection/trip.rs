use crate::models::TripReportState;

struct Counter {
    active: bool,
    count: i64,
    last: f64,
}
impl Default for Counter {
    fn default() -> Self {
        Self {
            active: false,
            count: 0,
            last: f64::NEG_INFINITY,
        }
    }
}
impl Counter {
    fn update(&mut self, time: f64, triggered: bool, held: bool) {
        if triggered && !self.active && time - self.last >= 1.5 {
            self.active = true;
            self.count += 1;
            self.last = time;
        } else if !held {
            self.active = false;
        }
    }
}

#[derive(Default)]
pub(super) struct Tracker {
    onroad: Option<bool>,
    last: Option<f64>,
    snapshot: TripReportState,
    auto_distance: f64,
    accel: Counter,
    brake: Counter,
    corner: Counter,
}
pub(super) struct Reading {
    pub time: f64,
    pub speed: f64,
    pub accel: f64,
    pub angle: Option<f64>,
    pub enabled: bool,
    pub wheelbase: f64,
    pub ratio: f64,
}
impl Tracker {
    pub fn set_onroad(&mut self, onroad: bool) {
        if onroad && self.onroad == Some(false) {
            *self = Self::default();
        }
        self.onroad = Some(onroad);
    }
    pub fn update(&mut self, reading: &Reading) -> TripReportState {
        if self.onroad == Some(false) || !reading.time.is_finite() {
            return self.snapshot.clone();
        }
        let speed = if reading.speed.is_finite() {
            reading.speed.max(0.0)
        } else {
            0.0
        };
        let accel = if reading.accel.is_finite() {
            reading.accel
        } else {
            0.0
        };
        let Some(last) = self.last else {
            self.last = Some(reading.time);
            return self.snapshot.clone();
        };
        let raw_dt = reading.time - last;
        if raw_dt <= 0.0 {
            return self.snapshot.clone();
        }
        self.last = Some(reading.time);
        let dt = raw_dt.min(0.5);
        self.snapshot.duration_s += dt;
        let angle = reading
            .angle
            .filter(|angle| angle.is_finite())
            .unwrap_or(0.0)
            .to_radians()
            / reading.ratio.max(1.0);
        let curvature = angle.tan() / reading.wheelbase.max(1.5);
        let distance = speed * dt;
        self.snapshot.distance_m += distance;
        if speed >= 0.3 {
            self.snapshot.moving_time_s += dt;
            if reading.enabled {
                self.auto_distance += distance;
            }
        }
        self.snapshot.max_speed_kph = self.snapshot.max_speed_kph.max(speed * 3.6);
        self.snapshot.max_accel_mps2 = self.snapshot.max_accel_mps2.max(accel);
        self.snapshot.max_decel_mps2 = self.snapshot.max_decel_mps2.min(accel);
        self.accel
            .update(reading.time, accel >= 2.5, accel >= 2.5 * 0.8);
        self.brake
            .update(reading.time, accel <= -3.0, accel <= -3.0 * 0.8);
        let lateral = (speed * speed * curvature).abs();
        self.corner.update(
            reading.time,
            speed >= 5.5 && lateral >= 3.0,
            speed >= 5.5 && lateral >= 3.0 * 0.8,
        );
        self.snapshot.hard_accel_count = self.accel.count;
        self.snapshot.hard_brake_count = self.brake.count;
        self.snapshot.hard_corner_count = self.corner.count;
        self.snapshot.average_speed_kph = if self.snapshot.moving_time_s > 0.0 {
            self.snapshot.distance_m / self.snapshot.moving_time_s * 3.6
        } else {
            0.0
        };
        self.snapshot.auto_ratio_percent = if self.snapshot.distance_m > 0.0 {
            self.auto_distance / self.snapshot.distance_m * 100.0
        } else {
            0.0
        };
        self.snapshot.clone()
    }
}
