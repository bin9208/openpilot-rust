use super::{Data, Projection};
use crate::{
    config::{MAX_ACCEL_MPS2, MAX_SPEED_KPH, MAX_STEERING_ANGLE_DEG, ROAD_CURVE_M_PER_M2},
    models::{ClusterUiState, CruiseDisplayState, TpmsInfo},
};
use capnp::dynamic_value::Reader;
use num_traits::ToPrimitive;

impl Projection {
    #[must_use]
    pub fn car_frame(&mut self, car: Reader<'_>, time: f64) -> ClusterUiState {
        let car = Data(car);
        let speed = self.movement(car);
        self.dashboard(car, time);
        self.state.left_signal = car.flag("leftBlinker", false);
        self.state.right_signal = car.flag("rightBlinker", false);
        self.state.left_blindspot = car.flag("leftBlindspot", false);
        self.state.right_blindspot = car.flag("rightBlindspot", false);
        self.state.throttle = car.number("gas", 0.0).clamp(0.0, 1.0);
        self.state.brake = car.number("brake", 0.0).clamp(0.0, 1.0);
        let pressures = car.field("tpms");
        let pressure = |name| {
            pressures
                .field(name)
                .float()
                .filter(|pressure| (5.0..=60.0).contains(pressure))
        };
        self.state.tpms = TpmsInfo {
            fl: pressure("fl"),
            fr: pressure("fr"),
            rl: pressure("rl"),
            rr: pressure("rr"),
        };
        self.state.ev_mode_valid = car.flag("evModeValid", false);
        self.state.ev_mode_active = self.state.ev_mode_valid && car.flag("evModeActive", false);
        self.state.trip_report = Some(
            self.trip.update(&super::trip::Reading {
                time,
                speed,
                accel: self.state.accel_mps2,
                angle: self.state.steering_angle_deg,
                enabled: self
                    .selfdrive_enabled
                    .unwrap_or(self.controls_enabled.unwrap_or(false)),
                wheelbase: self.wheelbase,
                ratio: self.steer_ratio,
            }),
        );
        self.lanes.car(car);
        let observed = self.lanes.center().map_or(0.0, |center| {
            (-center / self.state.lane_width_m.clamp(2.4, 4.6)).clamp(-1.25, 1.25)
        });
        let animation = self.change.frame(&mut self.state, time, observed);
        self.lanes.frame(&mut self.state, &animation);
        let model = if time - self.model_detection_time < 0.8 {
            self.model_detections.as_slice()
        } else {
            &[]
        };
        self.state.detected_vehicles = self.radar.visible_detections(
            model,
            car,
            time,
            self.lanes.edge_bounds(self.state.lane_width_m),
        );
        self.state.radar_points = self.radar.visible_points(time);
        self.state.corner_radar_supported = self.radar.active();
        (
            self.state.recorded_cutin_active,
            self.state.recorded_cutin_sound,
        ) = self.radar.recorded(time);
        self.state.clone()
    }
    fn movement(&mut self, car: Data<'_>) -> f64 {
        let speed = car.number("vEgo", 0.0).max(0.0);
        self.state.speed_kph = (speed * 3.6).clamp(0.0, MAX_SPEED_KPH);
        let cluster = car.number("vEgoCluster", 0.0);
        self.v_ego_cluster_seen |= cluster != 0.0;
        self.state.display_speed_kph = Some(
            ((if self.v_ego_cluster_seen {
                cluster
            } else {
                speed
            })
            .max(0.0)
                * 3.6)
                .clamp(0.0, MAX_SPEED_KPH),
        );
        self.state.accel_mps2 = car
            .number("aEgo", 0.0)
            .clamp(-MAX_ACCEL_MPS2, MAX_ACCEL_MPS2);
        self.state.steering_angle_deg = car.field("steeringAngleDeg").float();
        self.state.steering = self
            .model_curvature
            .or(self.controls_curvature)
            .map_or_else(
                || {
                    self.state.steering_angle_deg.map_or(0.0, |angle| {
                        (angle / MAX_STEERING_ANGLE_DEG).clamp(-1.0, 1.0)
                    })
                },
                |curvature| (curvature / (2.0 * ROAD_CURVE_M_PER_M2)).clamp(-1.0, 1.0),
            );
        speed
    }
    fn dashboard(&mut self, car: Data<'_>, time: f64) {
        self.state.cruise_kph = cruise(car);
        self.state.cruise_display_state =
            self.state.cruise_kph.map_or(CruiseDisplayState::Off, |_| {
                if let Some(enabled) = self.selfdrive_enabled.or(self.controls_enabled) {
                    if enabled {
                        CruiseDisplayState::Engaged
                    } else {
                        CruiseDisplayState::Off
                    }
                } else if car.field("cruiseState").flag("enabled", false) {
                    CruiseDisplayState::Engaged
                } else {
                    CruiseDisplayState::Paused
                }
            });
        self.state.gear_text = gear(car);
        if let Some(gap) = car
            .field("pcmCruiseGap")
            .int()
            .filter(|gap| (1..=4).contains(gap))
        {
            self.state.cruise_gap = Some(gap);
        }
        if time - self.nav_time > 10.0 {
            self.nav_limit = None;
            self.nav_time = -999.0;
        }
        let vehicle_limit = car
            .field("speedLimit")
            .float()
            .filter(|limit| *limit > 0.0)
            .and_then(|limit| limit.round_ties_even().to_i64());
        self.state.speed_limit_kph = vehicle_limit.or(self.nav_limit);
        self.state.speed_limit_source = if vehicle_limit.is_some() {
            Some("v".into())
        } else if self.nav_limit.is_some() {
            Some("n".into())
        } else {
            None
        };
    }
}

fn cruise(car: Data<'_>) -> Option<i64> {
    let cruise = car.field("cruiseState");
    if !cruise.flag("available", true) {
        return None;
    }
    for field in ["vCruiseCluster", "vCruise"] {
        let value = car.number(field, 0.0);
        if value > 0.0 && value < 250.0 {
            return value.round_ties_even().to_i64();
        }
    }
    for field in ["speedCluster", "speed"] {
        let value = cruise.number(field, 0.0);
        if value > 0.1 && value < 70.0 {
            return (value * 3.6).round_ties_even().to_i64();
        }
    }
    None
}
fn gear(car: Data<'_>) -> Option<String> {
    let gear = car.field("gearShifter").text()?.to_lowercase();
    if gear.is_empty() {
        return None;
    }
    if gear.contains("drive") {
        return Some(
            car.field("gearStep")
                .int()
                .filter(|step| (1..=8).contains(step))
                .map_or_else(|| "D".into(), |step| step.to_string()),
        );
    }
    for (name, symbol) in [
        ("park", "P"),
        ("reverse", "R"),
        ("neutral", "N"),
        ("sport", "S"),
        ("low", "L"),
        ("brake", "B"),
        ("eco", "E"),
        ("unknown", "U"),
    ] {
        if gear.contains(name) {
            return Some(symbol.into());
        }
    }
    Some("M".into())
}
