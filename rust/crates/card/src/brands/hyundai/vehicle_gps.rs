//! Receive-only vehicle-position continuity; not an OEM satellite-fix flag.
use openpilot_can::dbc::Dbc;
use openpilot_cereal::car_capnp::car_state;
const TIMEOUT: f64 = 2.5;
pub const MESSAGE: &str = "VEHICLE_POSITION_374";

pub fn supported(dbc: &Dbc) -> bool {
    let Ok(message) = dbc.message(MESSAGE) else {
        return false;
    };
    message.address == 0x374
        && message.size == 8
        && [("LATITUDE", 0), ("LONGITUDE", 32)]
            .iter()
            .all(|(name, bit)| {
                message.signals.iter().any(|s| {
                    s.name == *name
                        && s.start_bit == *bit
                        && s.size == 32
                        && s.little_endian
                        && !s.signed
                        && s.factor == 1e-6
                        && s.offset == 0.
                })
            })
}
#[derive(Debug, Default)]
pub struct Fix {
    pub available: bool,
    pub valid: bool,
    pub latitude: f64,
    pub longitude: f64,
    pub sample_mono_time: u64,
    pub heading_deg: f64,
    pub heading_valid: bool,
}
impl Fix {
    pub fn write(&self, mut out: car_state::vehicle_gps::Builder<'_>) {
        out.set_available(self.available);
        out.set_valid(self.valid);
        out.set_latitude(self.latitude);
        out.set_longitude(self.longitude);
        out.set_sample_mono_time(self.sample_mono_time);
        out.set_heading_deg(self.heading_deg as f32);
        out.set_heading_valid(self.heading_valid);
    }
}
#[derive(Default)]
pub struct VehicleGps {
    stamp: u64,
    tick: u64,
    heading_stamp: u64,
    point: Option<[f64; 2]>,
    runs: u8,
    travel: f64,
    frozen_travel: f64,
    heading: f64,
    accepted: bool,
    frozen: bool,
}
fn age(now: u64, then: u64) -> f64 {
    (now as i128 - then as i128) as f64 / 1e9
}
fn displacement(a: [f64; 2], b: [f64; 2]) -> (f64, f64) {
    let dy = (b[0] - a[0]).to_radians() * 6_371_000.;
    let dx = (b[1] - a[1]).to_radians() * 6_371_000. * ((a[0] + b[0]) / 2.).to_radians().cos();
    (dx.hypot(dy), dx.atan2(dy).to_degrees().rem_euclid(360.))
}
impl VehicleGps {
    pub fn update(
        &mut self,
        now: u64,
        stamp: u64,
        bus_timeout: bool,
        raw: &[u8],
        speed: f64,
        reverse: bool,
    ) -> Fix {
        let dt = if self.tick > 0 {
            age(now, self.tick)
        } else {
            0.
        };
        self.tick = now;
        let speed_ok = speed.is_finite() && (0. ..=90.).contains(&speed);
        if !(0. ..=TIMEOUT).contains(&dt) || !speed_ok {
            self.runs = 0;
            self.accepted = false;
        }
        if speed_ok && (0. ..=TIMEOUT).contains(&dt) {
            self.travel += speed * dt;
            self.frozen_travel += speed * dt;
        }
        let fresh = stamp > 0 && (0. ..=TIMEOUT).contains(&age(now, stamp)) && !bus_timeout;
        if stamp != self.stamp {
            let elapsed = age(stamp, self.stamp);
            self.stamp = stamp;
            let point = if raw.len() == 8 {
                [
                    u32::from_le_bytes(raw[..4].try_into().unwrap()) as f64 / 1e6,
                    u32::from_le_bytes(raw[4..].try_into().unwrap()) as f64 / 1e6,
                ]
            } else {
                [0., 0.]
            };
            let layout = raw.len() == 8
                && point[0] > 0.
                && point[0] <= 85.05112878
                && point[1] > 0.
                && point[1] <= 180.;
            let (distance, course) = self
                .point
                .filter(|_| layout)
                .map_or((0., 0.), |old| displacement(old, point));
            let consistent = self.point.is_some()
                && (0.5..=TIMEOUT).contains(&elapsed)
                && distance <= (90. * elapsed).min(15. + 2. * self.travel);
            if layout && fresh && speed_ok {
                self.runs = if consistent {
                    (self.runs + 1).min(3)
                } else {
                    1
                };
                if self.point != Some(point) {
                    self.frozen_travel = 0.;
                }
                self.frozen = self.frozen_travel > 15.;
                if consistent && self.runs >= 3 && distance >= 5. && speed >= 2. && !reverse {
                    self.heading = course;
                    self.heading_stamp = stamp;
                } else if !consistent {
                    self.heading_stamp = 0;
                }
                self.point = Some(point);
                self.accepted = self.runs >= 3;
            } else {
                self.point = None;
                self.runs = 0;
                self.heading_stamp = 0;
                self.accepted = false;
            }
            self.travel = 0.;
        }
        let valid = fresh && self.accepted && speed_ok && !self.frozen;
        if !fresh {
            self.runs = 0;
            self.accepted = false;
            self.heading_stamp = 0;
        }
        let heading_valid = valid
            && !reverse
            && self.heading_stamp > 0
            && (0. ..=if speed < 0.5 { 30. } else { TIMEOUT })
                .contains(&age(now, self.heading_stamp));
        let point = self.point.unwrap_or([0., 0.]);
        Fix {
            available: self.stamp > 0,
            valid,
            latitude: point[0],
            longitude: point[1],
            sample_mono_time: self.stamp,
            heading_deg: self.heading,
            heading_valid,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dbc_profile_is_verified_by_layout_and_not_vehicle_name() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../opendbc_repo/opendbc/dbc/hyundai_canfd_generated.dbc");
        let source = std::fs::read_to_string(path).unwrap();
        assert!(supported(&Dbc::parse("any-car", &source).unwrap()));
        for source in [
            source.replace(
                "BO_ 884 VEHICLE_POSITION_374: 8",
                "BO_ 885 VEHICLE_POSITION_374: 8",
            ),
            source.replace(
                "SG_ LATITUDE : 0|32@1+ (0.000001,0)",
                "SG_ LATITUDE : 0|32@1- (0.000001,0)",
            ),
        ] {
            assert!(!supported(
                &Dbc::parse("HYUNDAI IONIQ 5 PE", &source).unwrap()
            ));
        }
    }
    fn bytes(lat: f64, lon: f64) -> Vec<u8> {
        [(lat * 1e6).round() as u32, (lon * 1e6).round() as u32]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect()
    }
    #[test]
    fn requires_three_consistent_samples_and_rejects_stale_and_frozen_positions() {
        let mut gps = VehicleGps::default();
        for second in 1..=3 {
            let fix = gps.update(
                second * 1_000_000_000,
                second * 1_000_000_000,
                false,
                &bytes(37. + second as f64 * 0.0001, 127.),
                10.,
                false,
            );
            assert_eq!(fix.valid, second == 3);
            assert_eq!(fix.heading_valid, second == 3);
        }
        let last = bytes(37.0003, 127.);
        assert!(
            !gps.update(6_000_000_001, 3_000_000_000, false, &last, 10., false)
                .valid
        );
        assert!(
            !gps.update(7_000_000_000, 7_000_000_000, false, &last, 10., false)
                .valid
        );
        assert!(
            !gps.update(8_000_000_000, 8_000_000_000, false, &last, 10., false)
                .valid
        );
    }
    #[test]
    fn highway_motion_between_samples_is_not_a_frozen_coordinate() {
        let mut gps = VehicleGps::default();
        for second in 1..=4 {
            let fix = gps.update(
                second * 1_000_000_000,
                second * 1_000_000_000,
                false,
                &bytes(37. + second as f64 * 0.0003, 127.),
                33.,
                false,
            );
            assert_eq!(fix.valid, second >= 3);
        }
        let fix = gps.update(
            5_000_000_000,
            5_000_000_000,
            false,
            &bytes(37.0015, 127.),
            33.,
            true,
        );
        assert!(fix.valid);
        assert!(!fix.heading_valid);
    }
    #[test]
    fn invalid_speed_raw_length_jump_or_bus_timeout_cannot_produce_a_fix() {
        for (raw, speed, timeout) in [
            (bytes(37., 127.), f64::NAN, false),
            (vec![0; 7], 0., false),
            (bytes(37., 127.), 0., true),
            (bytes(0., 127.), 0., false),
        ] {
            let fix = VehicleGps::default().update(
                1_000_000_000,
                1_000_000_000,
                timeout,
                &raw,
                speed,
                false,
            );
            assert!(!fix.valid);
        }
    }
}
