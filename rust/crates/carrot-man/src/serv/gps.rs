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
        if !input.car_updated || !input.control_updated {
            return self.gps.angle;
        }
        let (speed, now) = motion;
        self.gps.valid = input.gps_updated && input.has_fix;
        let phone = now - self.gps.last_phone < 3.;
        let navi = now - self.gps.last_navi < 3.;
        let bearing = if navi {
            self.gps.angle
        } else if phone {
            self.gps.phone_angle
        } else if self.gps.valid {
            self.gps.angle = input.bearing_deg;
            input.bearing_deg
        } else {
            self.gps.angle
        };
        self.gps.offset = 0.;
        if self.gps.valid && !(phone || navi) {
            self.gps.navi_latitude = input.latitude;
            self.gps.navi_longitude = input.longitude;
            self.gps.last_calculate = now;
        } else if navi {
            if (self.gps.measured - bearing).abs() < 0.1 {
                self.gps.diff_angle_count += 1;
            } else {
                self.gps.diff_angle_count = 0;
            }
            self.gps.measured = bearing;
            if self.gps.diff_angle_count > 5 {
                let mut difference = (self.gps.angle - bearing).rem_euclid(360.);
                if difference > 180. {
                    difference -= 360.;
                }
                self.gps.offset = self.gps.offset * 0.9 + difference * 0.1;
            }
        }
        let calculated = (bearing + self.gps.offset).rem_euclid(360.);
        let dt = now - self.gps.last_calculate;
        (self.gps.latitude, self.gps.longitude) = if dt > 5. {
            (0., 0.)
        } else if dt == 0. {
            (self.gps.navi_latitude, self.gps.navi_longitude)
        } else {
            estimate_position(
                (self.gps.navi_latitude, self.gps.navi_longitude),
                (speed, calculated),
                dt,
            )
        };
        calculated
    }
}
