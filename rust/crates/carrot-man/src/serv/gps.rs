use super::{CarrotServ, GpsInput};

pub fn estimate_position(position: (f64, f64), motion: (f64, f64), dt: f64) -> (f64, f64) {
    let distance = motion.0 * dt;
    let angle = motion.1.to_radians();
    (
        position.0 + (distance * angle.cos() / 6_371_000.).to_degrees(),
        position.1
            + (distance * angle.sin() / (6_371_000. * position.0.to_radians().cos())).to_degrees(),
    )
}

impl CarrotServ {
    pub fn update_gps(&mut self, input: &GpsInput, motion: (f64, f64)) -> f64 {
        use super::gps_sources::{Fix, Source};
        let (speed, now) = motion;
        if input.gps_updated {
            if input.gps_valid && input.has_fix {
                self.gps.sources.put(
                    Source::Device,
                    Fix {
                        latitude: input.latitude,
                        longitude: input.longitude,
                        heading: Some(input.bearing_deg),
                        speed: input.gps_speed,
                        stamp: input.gps_received,
                    },
                );
            } else {
                self.gps.sources.remove(Source::Device);
            }
        }
        if !input.gps_alive || !input.gps_valid {
            self.gps.sources.remove(Source::Device);
        }
        let ego_speed = if speed.is_finite() && (0. ..=90.).contains(&speed) {
            speed
        } else {
            0.
        };
        if input.car_valid && input.vehicle_valid {
            self.gps.sources.put(
                Source::Vehicle,
                Fix {
                    latitude: input.vehicle_latitude,
                    longitude: input.vehicle_longitude,
                    heading: input.vehicle_heading,
                    speed: ego_speed,
                    stamp: input.vehicle_stamp,
                },
            );
        } else {
            self.gps.sources.remove(Source::Vehicle);
        }
        let selected = self.gps.sources.select(now);
        self.gps.position_source = selected.map_or("none", |(source, _)| source.name());
        self.gps.position_mono_time = now;
        self.gps.position_heading_valid = selected.is_some_and(|(_, fix)| fix.heading.is_some());
        let Some((_, fix)) = selected else {
            self.gps.latitude = 0.;
            self.gps.longitude = 0.;
            self.gps.last_calculate = 0.;
            return self.gps.angle;
        };
        self.gps.last_calculate = fix.stamp;
        if let Some(heading) = fix.heading {
            self.gps.angle = heading;
        }
        self.gps.speed = fix.speed * 3.6;
        let elapsed = if fix.heading.is_some() {
            now - fix.stamp
        } else {
            0.
        };
        (self.gps.latitude, self.gps.longitude) = estimate_position(
            (fix.latitude, fix.longitude),
            (ego_speed, self.gps.angle),
            elapsed,
        );
        self.gps.angle
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serv::{
        gps_sources::{Fix, Source},
        settings::Settings,
    };
    #[test]
    fn owner_handover_keeps_receipts_and_unknown_vehicle_course_cannot_project() {
        let settings:Settings=serde_json::from_value(serde_json::json!({"bump_speed":0.,"bump_time":0.,"bump_end_distance":0.,"control_end":0.,"rear_hold_distance":0.,"control_mode":0,"vehicle_can_control":0,"school_control":false,"camera_control_mode":0,"safety_factor":0.,"deceleration":0.,"countdown_mode":0,"turn_speed_mode":0,"map_turn_factor":0.,"auto_turn_speed":0.,"auto_turn_map_change":0,"auto_turn_control":0,"auto_turn_end":0.,"curve_lower_limit":0.,"is_metric":true,"road_limit_offset":0.,"curve_factor":0.,"language":"en"})).unwrap();
        let mut serv = CarrotServ::new(settings);
        let mut input = GpsInput {
            gps_alive: true,
            gps_valid: true,
            gps_updated: true,
            has_fix: true,
            gps_received: 10.,
            latitude: 38.,
            longitude: 128.,
            bearing_deg: 180.,
            car_valid: true,
            vehicle_valid: true,
            vehicle_latitude: 37.,
            vehicle_longitude: 127.,
            vehicle_stamp: 10.,
            vehicle_heading: Some(90.),
            ..GpsInput::default()
        };
        for (source, latitude, longitude) in [
            (Source::ExternalNavi, 36., 126.),
            (Source::Phone, 35., 125.),
        ] {
            serv.gps.sources.put(
                source,
                Fix {
                    latitude,
                    longitude,
                    heading: Some(45.),
                    speed: 0.,
                    stamp: 10.,
                },
            );
        }
        serv.update_gps(&input, (0., 10.));
        assert_eq!(serv.gps.position_source, "external_navi");
        serv.gps.sources.remove(Source::ExternalNavi);
        input.gps_updated = false;
        serv.update_gps(&input, (0., 10.5));
        assert_eq!(serv.gps.position_source, "phone");
        assert_eq!(serv.gps.last_calculate, 10.);
        serv.gps.sources.remove(Source::Phone);
        input.vehicle_heading = None;
        serv.update_gps(&input, (20., 11.));
        assert_eq!(serv.gps.position_source, "vehicle");
        assert!(!serv.gps.position_heading_valid);
        assert_eq!((serv.gps.latitude, serv.gps.longitude), (37., 127.));
        input.vehicle_valid = false;
        serv.update_gps(&input, (0., 11.));
        assert_eq!(serv.gps.position_source, "device");
        serv.update_gps(&input, (f64::NAN, 13.));
        assert_eq!(serv.gps.position_source, "none");
        assert_eq!(serv.gps.latitude, 0.);
    }
}
