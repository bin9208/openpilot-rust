use crate::{base::Base, data::Data, point::Point, reader::Reader, Error};

#[derive(Default)]
pub struct Rivian {
    pub track_id: u64,
}

impl Rivian {
    pub fn update(&mut self, base: &mut Base, reader: &mut Reader) -> Result<Data, Error> {
        let mut data = Data::default();
        data.errors.can_error = !reader.parser.can_valid();
        for address in 0x500..0x520 {
            let id = u64::from(address);
            if !base.pts.contains_key(&id) {
                base.pts.insert(
                    id,
                    Point {
                        track_id: self.track_id,
                        ..Point::default()
                    },
                );
                self.track_id += 1;
            }
            let state = reader.signal(address, "STATE")?;
            if (state == 3. || state == 4.) && reader.signal(address, "STATE_2")? == 1. {
                let azimuth = reader.signal(address, "AZIMUTH")? * (std::f64::consts::PI / 180.);
                let distance = reader.signal(address, "LONG_DIST")?;
                let point = base
                    .pts
                    .get_mut(&id)
                    .ok_or(Error::Contract("Rivian point absent"))?;
                point.measured = true;
                point.d_rel = (azimuth.cos() * distance) as f32;
                point.y_rel = (0.5 * -azimuth.sin() * distance) as f32;
                point.v_rel = reader.signal(address, "REL_SPEED")? as f32;
                point.a_rel = f32::NAN;
                point.yv_rel = 0.;
            } else {
                base.pts.shift_remove(&id);
            }
        }
        data.points_present = true;
        data.points = base.pts.values().cloned().collect();
        Ok(data)
    }
}
