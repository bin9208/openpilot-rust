use crate::{base::Base, data::Data, point::Point, reader::Reader, Error};

#[derive(Default)]
pub struct Tesla {
    pub track_id: u64,
}

pub fn messages() -> Vec<(u32, f64)> {
    [0x401]
        .into_iter()
        .chain(0x410..0x460)
        .map(|a| (a, 16.))
        .collect()
}

impl Tesla {
    pub fn update(&mut self, base: &mut Base, reader: &mut Reader) -> Result<Data, Error> {
        let mut data = Data::default();
        data.errors.can_error = !reader.parser.can_valid();
        data.errors.radar_unavailable_temporary =
            reader.signal(0x401, "shortTermUnavailable")? != 0.;
        data.errors.radar_fault = reader.signal(0x401, "sensorBlocked")? != 0.
            || reader.signal(0x401, "vehDynamicsError")? != 0.;
        for index in 0..40 {
            let a = 0x410 + 2 * index;
            let b = a + 1;
            if reader.signal(a, "Index")? != reader.signal(b, "Index2")? {
                continue;
            }
            let id = u64::from(index);
            if reader.signal(a, "Tracked")? == 0. {
                base.pts.shift_remove(&id);
                continue;
            }
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
            let point = base
                .pts
                .get_mut(&id)
                .ok_or(Error::Contract("Tesla point absent"))?;
            point.d_rel = reader.signal(a, "LongDist")? as f32;
            point.y_rel = reader.signal(a, "LatDist")? as f32;
            point.v_rel = reader.signal(a, "LongSpeed")? as f32;
            point.a_rel = reader.signal(a, "LongAccel")? as f32;
            point.yv_rel = reader.signal(b, "LatSpeed")? as f32;
            point.measured = reader.signal(a, "Meas")? != 0.;
        }
        data.points_present = true;
        data.points = base.pts.values().cloned().collect();
        Ok(data)
    }
}
